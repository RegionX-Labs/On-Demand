//! This file contains all the configuration related traits.

use crate::RelayBlockNumber;
use codec::{Codec, Decode};
use cumulus_primitives_core::ConsensusEngineId;
use cumulus_relay_chain_interface::{PHash, RelayChainInterface};
use on_demand_primitives::{OnDemandRuntimeApi, ThresholdParameterT};
use sc_client_api::UsageProvider;
use sc_service::Arc;
use sc_transaction_pool_api::MaintainedTransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::RuntimeAppPublic;
use sp_consensus_aura::{AuraApi, Slot, AURA_ENGINE_ID};
use sp_core::crypto::Pair as PairT;
use sp_runtime::traits::{
	AtLeast32BitUnsigned, Block as BlockT, Debug, Header as HeaderT, MaybeDisplay, Member,
	PhantomData,
};
use std::{error::Error, fmt::Display};

pub trait OnDemandConfig {
	/// Custom order placement criteria.
	type OrderPlacementCriteria: OrderCriteria;

	/// Author identifier.
	type AuthorPub: Member + RuntimeAppPublic + Display + Send;

	/// Block type.
	type Block: BlockT;

	/// Relay chain.
	type R: RelayChainInterface + Clone;

	/// Parachain.
	type P: ProvideRuntimeApi<Self::Block> + UsageProvider<Self::Block> + Send + Sync;

	/// Extrinsic pool.
	type ExPool: MaintainedTransactionPool<Block = Self::Block, Hash = <Self::Block as BlockT>::Hash>
		+ 'static;

	/// Balance type.
	type Balance: Codec
		+ MaybeDisplay
		+ 'static
		+ Debug
		+ Send
		+ Into<u128>
		+ AtLeast32BitUnsigned
		+ Copy
		+ From<u128>;

	/// On-demand pallet threshold parameter.
	type ThresholdParameter: ThresholdParameterT;

	fn order_placer(
		para: &Self::P,
		relay_hash: PHash,
		para_header: <Self::Block as BlockT>::Header,
	) -> Result<Self::AuthorPub, Box<dyn Error>>;
}

pub trait OrderCriteria {
	type Block: BlockT;
	type P: ProvideRuntimeApi<Self::Block> + UsageProvider<Self::Block>;
	type ExPool: MaintainedTransactionPool<Block = Self::Block, Hash = <Self::Block as BlockT>::Hash>
		+ 'static;

	/// Returns true or false depending on whether an order should be placed.
	fn should_place_order(
		parachain: &Self::P,
		transaction_pool: Arc<Self::ExPool>,
		height: RelayBlockNumber,
	) -> bool;
}

pub struct OnDemandSlot<R, P, Block, Pair, ExPool, Balance, C, T>(
	PhantomData<(R, P, Block, Pair, ExPool, Balance, C, T)>,
);
#[async_trait::async_trait]
impl<P, R, Block, Pair, ExPool, Balance, Criteria, Threshold> OnDemandConfig
	for OnDemandSlot<R, P, Block, Pair, ExPool, Balance, Criteria, Threshold>
where
	R: RelayChainInterface + Clone + Sync + Send,
	P: ProvideRuntimeApi<Block> + UsageProvider<Block> + Sync + Send,
	P::Api: AuraApi<Block, Pair::Public>
		+ OnDemandRuntimeApi<Block, Balance, RelayBlockNumber, Threshold>,
	Criteria: OrderCriteria,
	Pair: PairT + 'static,
	ExPool: MaintainedTransactionPool<Block = Block, Hash = <Block as BlockT>::Hash> + 'static,
	Balance: Codec
		+ MaybeDisplay
		+ 'static
		+ Debug
		+ Send
		+ Into<u128>
		+ AtLeast32BitUnsigned
		+ Copy
		+ From<u128>,
	Pair::Public: RuntimeAppPublic + Display + Member + Codec,
	Block: BlockT,
	<<Block as BlockT>::Header as HeaderT>::Number: Into<u128>,
	Threshold: ThresholdParameterT,
{
	type P = P;
	type R = R;

	type OrderPlacementCriteria = Criteria;
	type AuthorPub = Pair::Public;
	type Block = Block;

	type ExPool = ExPool;
	type Balance = Balance;
	type ThresholdParameter = Threshold;

	fn order_placer(
		para: &P,
		_relay_hash: PHash,
		para_header: <Self::Block as BlockT>::Header,
	) -> Result<Self::AuthorPub, Box<dyn Error>> {
		let para_hash = para_header.hash();
		let para_height: u128 = para_header.number().clone().into();
		let authorities = para.runtime_api().authorities(para_hash).map_err(Box::new)?;
		let slot_width = para.runtime_api().slot_width(para_hash).map_err(Box::new)?;

		let indx = (para_height >> slot_width) % authorities.len() as u128;
		let author = authorities.get(indx as usize).ok_or("Failed to get selected collator")?;

		Ok(author.clone())
	}
}

pub struct OnDemandAura<R, P, Block, Pair, ExPool, Balance, C, T>(
	PhantomData<(R, P, Block, Pair, ExPool, Balance, C, T)>,
);
#[async_trait::async_trait]
impl<P, R, Block, Pair, ExPool, Balance, Criteria, Threshold> OnDemandConfig
	for OnDemandAura<R, P, Block, Pair, ExPool, Balance, Criteria, Threshold>
where
	R: RelayChainInterface + Clone + Sync + Send,
	P: ProvideRuntimeApi<Block> + UsageProvider<Block> + Sync + Send,
	P::Api: AuraApi<Block, Pair::Public>,
	Criteria: OrderCriteria,
	Pair: PairT + 'static,
	ExPool: MaintainedTransactionPool<Block = Block, Hash = <Block as BlockT>::Hash> + 'static,
	Balance: Codec
		+ MaybeDisplay
		+ 'static
		+ Debug
		+ Send
		+ Into<u128>
		+ AtLeast32BitUnsigned
		+ Copy
		+ From<u128>,
	Pair::Public: RuntimeAppPublic + Display + Member + Codec,
	Block: BlockT,
	Threshold: ThresholdParameterT,
{
	type P = P;
	type R = R;

	type OrderPlacementCriteria = Criteria;
	type AuthorPub = Pair::Public;
	type Block = Block;

	type ExPool = ExPool;
	type Balance = Balance;
	type ThresholdParameter = Threshold;

	fn order_placer(
		para: &P,
		_relay_hash: PHash,
		para_header: <Self::Block as BlockT>::Header,
	) -> Result<Self::AuthorPub, Box<dyn Error>> {
		let para_hash = para_header.hash();
		let authorities = para.runtime_api().authorities(para_hash).map_err(Box::new)?;

		let author_index = find_author(
			para_header.digest().logs().iter().filter_map(|d| d.as_pre_runtime()),
			authorities.len(),
		)
		.ok_or("Could not find aura author index")?;

		let author = authorities.get(author_index as usize).ok_or("Invalid aura index")?;
		Ok(author.clone())
	}
}

fn find_author<'a, I>(digests: I, authorities_len: usize) -> Option<u32>
where
	I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
{
	for (id, mut data) in digests.into_iter() {
		if id == AURA_ENGINE_ID {
			let slot = Slot::decode(&mut data).ok()?;
			let author_index = *slot % authorities_len as u64;
			return Some(author_index as u32)
		}
	}

	None
}
