//! On-demand order placement service.
//!
//! NOTE: Inspiration was taken from the Magnet(https://github.com/Magport/Magnet) on-demand integration.

use crate::{
	chain::{
		get_spot_price, is_parathread, on_demand_cores_available,
		polkadot::runtime_types::{frame_system::AccountInfo, pallet_balances::types::AccountData},
	},
	config::{OnDemandConfig, OrderCriteria},
};
use codec::Decode;
use cumulus_primitives_core::{
	relay_chain::{BlockNumber as RelayBlockNumber, Nonce},
	ParaId, PersistedValidationData,
};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::{pin_mut, select, FutureExt, Stream, StreamExt};
use order_primitives::{
	well_known_keys::{account, session_key_owner, ON_DEMAND_QUEUE},
	EnqueuedOrder,
};
use polkadot_primitives::{AccountId, OccupiedCoreAssumption};
use sc_service::TaskManager;
use sp_core::H256;
use sp_keystore::KeystorePtr;
use sp_runtime::{
	traits::{Block as BlockT, Header},
	RuntimeAppPublic,
};
use std::{error::Error, net::SocketAddr, sync::Arc};

mod chain;
pub mod config;

const LOG_TARGET: &str = "order-service";

/// Start all the on-demand order creation related tasks.
pub fn start_on_demand<Config>(
	parachain: Arc<Config::P>,
	para_id: ParaId,
	relay_chain: Config::R,
	transaction_pool: Arc<Config::ExPool>,
	task_manager: &TaskManager,
	keystore: KeystorePtr,
	relay_rpc: Option<SocketAddr>,
	rc_balance_baseline: Config::Balance,
) -> sc_service::error::Result<()>
where
	Config: OnDemandConfig + 'static,
	Config::OrderPlacementCriteria:
		OrderCriteria<P = Config::P, Block = Config::Block, ExPool = Config::ExPool>,
{
	let mut url = String::from("ws://"); // <- TODO wss
	url.push_str(
		&relay_rpc
			.expect("RPC address required for submitting on-demand orders")
			.to_string(),
	);

	let on_demand_task = run_on_demand_task::<Config>(
		para_id,
		parachain,
		relay_chain,
		keystore,
		transaction_pool,
		url,
		rc_balance_baseline,
	);

	task_manager.spawn_essential_handle().spawn_blocking(
		"on-demand order placement task",
		"on-demand",
		on_demand_task,
	);

	Ok(())
}

async fn run_on_demand_task<Config>(
	para_id: ParaId,
	parachain: Arc<Config::P>,
	relay_chain: Config::R,
	keystore: KeystorePtr,
	transaction_pool: Arc<Config::ExPool>,
	relay_url: String,
	rc_balance_baseline: Config::Balance,
) where
	Config: OnDemandConfig + 'static,
	Config::OrderPlacementCriteria:
		OrderCriteria<P = Config::P, Block = Config::Block, ExPool = Config::ExPool>,
{
	log::info!(
		target: LOG_TARGET,
		"Starting on-demand task"
	);

	let relay_chain_notification = follow_relay_chain::<Config>(
		para_id,
		parachain,
		relay_chain,
		keystore,
		transaction_pool,
		relay_url,
		rc_balance_baseline,
	);

	// let event_notification = event_notification(para_id, url, order_record);
	select! {
		_ = relay_chain_notification.fuse() => {},
		// _ = event_notification.fuse() => {},
	}
}

async fn follow_relay_chain<Config>(
	para_id: ParaId,
	parachain: Arc<Config::P>,
	relay_chain: Config::R,
	keystore: KeystorePtr,
	transaction_pool: Arc<Config::ExPool>,
	relay_url: String,
	rc_balance_baseline: Config::Balance,
) where
	Config: OnDemandConfig + 'static,
	Config::OrderPlacementCriteria:
		OrderCriteria<P = Config::P, Block = Config::Block, ExPool = Config::ExPool>,
{
	let new_best_heads = match new_best_heads(relay_chain.clone(), para_id).await {
		Ok(best_heads_stream) => best_heads_stream.fuse(),
		Err(_err) => {
			log::error!(
				target: LOG_TARGET,
				"Error: {:?}",
				_err
			);
			return;
		},
	};

	pin_mut!(new_best_heads);
	loop {
		select! {
			h = new_best_heads.next() => {
				match h {
					Some((height, validation_data, r_hash)) => {
						log::info!(
							target: LOG_TARGET,
							"New best relay head: {}",
							r_hash,
						);

						let _ = handle_relaychain_stream::<Config>(
							validation_data,
							height,
							&*parachain,
							keystore.clone(),
							transaction_pool.clone(),
							relay_chain.clone(),
							r_hash,
							para_id,
							relay_url.clone(),
							rc_balance_baseline,
						).await;
					},
					None => {
						return;
					}
				}
			},
		}
	}
}

/// Order placement logic
async fn handle_relaychain_stream<Config>(
	validation_data: PersistedValidationData,
	relay_height: RelayBlockNumber,
	parachain: &Config::P,
	keystore: KeystorePtr,
	transaction_pool: Arc<Config::ExPool>,
	relay_chain: Config::R,
	r_hash: H256,
	para_id: ParaId,
	relay_url: String,
	rc_balance_baseline: Config::Balance,
) -> Result<(), Box<dyn Error>>
where
	Config: OnDemandConfig + 'static,
	Config::OrderPlacementCriteria:
		OrderCriteria<P = Config::P, Block = Config::Block, ExPool = Config::ExPool>,
{
	let is_parathread = is_parathread(&relay_chain, r_hash, para_id).await?;

	if !is_parathread {
		log::info!(
			target: LOG_TARGET,
			"Not a parathread, relying on bulk coretime",
		);

		return Ok(());
	}

	let available = on_demand_cores_available(&relay_chain, r_hash)
		.await
		.ok_or("Failed to check if there are on-demand cores available")?;

	if available {
		log::info!(
			target: LOG_TARGET,
			"No cores allocated to on-demand"
		);

		return Ok(());
	}

	let head_encoded = validation_data.clone().parent_head.0;
	let para_head = <<Config::Block as BlockT>::Header>::decode(&mut &head_encoded[..])?;

	for acc in keystore.keys(sp_application_crypto::key_types::AURA)?.iter() {
		// Check if any of the accounts is below the baseline balance.

		let key_owner_storage = relay_chain
			.get_storage_by_key(
				para_head.hash(),
				session_key_owner(sp_application_crypto::key_types::AURA, acc.clone()).as_slice(),
			)
			.await?;
		let key_owner =
			key_owner_storage.map(|raw| <AccountId>::decode(&mut &raw[..])).transpose()?;

		// TODO: get the account id from the session pallet.
		let rc_account_storage =
			relay_chain.get_storage_by_key(r_hash, &account(key_owner)).await?;
		if let Some(rc_account_storage) = rc_account_storage {
			let rc_account: AccountInfo<Nonce, AccountData<Config::Balance>> =
				AccountInfo::decode(&mut &rc_account_storage[..])?;

			if rc_account.data.free <= rc_balance_baseline {
				log::warn!(
					target: LOG_TARGET,
					"Low relay chain balance: {}",
					rc_account.data.free
				)
			}
		}
	}

	let order_placer = Config::order_placer(parachain, r_hash, para_head)?.clone();

	if !keystore.has_keys(&[(order_placer.to_raw_vec(), sp_application_crypto::key_types::AURA)]) {
		// Expected author is not in the keystore therefore we are not responsible for order
		// creation.
		log::info!(
			target: LOG_TARGET,
			"Waiting for {} to create an order",
			order_placer
		);
		return Ok(());
	}

	let on_demand_queue_storage = relay_chain.get_storage_by_key(r_hash, ON_DEMAND_QUEUE).await?;
	let on_demand_queue = on_demand_queue_storage
		.map(|raw| <Vec<EnqueuedOrder>>::decode(&mut &raw[..]))
		.transpose()?;

	let order_exists = if let Some(queue) = on_demand_queue {
		queue.into_iter().position(|e| e.para_id == para_id).is_some()
	} else {
		false
	};

	if order_exists {
		return Ok(());
	}

	// Before placing an order ensure that the criteria for placing an order has been reached.
	let order_criteria_reached = Config::OrderPlacementCriteria::should_place_order(
		parachain,
		transaction_pool,
		relay_height,
	);

	if !order_criteria_reached {
		return Ok(());
	}

	let spot_price = get_spot_price::<Config::Balance>(relay_chain, r_hash)
		.await
		.ok_or("Failed to get spot price")?;

	log::info!(
		target: LOG_TARGET,
		"Placing an order",
	);

	chain::submit_order(&relay_url, para_id, spot_price.into(), keystore).await?;

	Ok(())
}

async fn new_best_heads(
	relay_chain: impl RelayChainInterface + Clone,
	para_id: ParaId,
) -> RelayChainResult<impl Stream<Item = (u32, PersistedValidationData, H256)>> {
	let new_best_notification_stream =
		relay_chain.new_best_notification_stream().await?.filter_map(move |n| {
			let relay_chain = relay_chain.clone();
			async move {
				let data = validation_data(relay_chain, n.hash(), para_id).await?;
				Some((n.number, data, n.hash()))
			}
		});

	Ok(new_best_notification_stream)
}

async fn validation_data(
	relay_chain: impl RelayChainInterface + Clone,
	hash: H256,
	para_id: ParaId,
) -> Option<PersistedValidationData> {
	relay_chain
		.persisted_validation_data(hash, para_id, OccupiedCoreAssumption::TimedOut)
		.await
		.map(|s| s.map(|s| s))
		.ok()
		.flatten()
}
