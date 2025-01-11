#![cfg_attr(not(feature = "std"), no_std)]

use crate::well_known_keys::EVENTS;
use codec::{Codec, Decode, Encode, MaxEncodedLen};
use cumulus_primitives_core::{relay_chain::BlockId, ParaId};
use cumulus_relay_chain_interface::{PHash, RelayChainInterface};
use frame_support::{pallet_prelude::InherentIdentifier, Parameter};
use scale_info::TypeInfo;
use sp_runtime::traits::{MaybeDisplay, MaybeSerializeDeserialize, Member};

pub mod well_known_keys;

// Identifier of the order inherent
pub const ON_DEMAND_INHERENT_IDENTIFIER: InherentIdentifier = *b"orderiht";

#[derive(Encode, Decode, Debug, PartialEq, Clone)]
pub struct EnqueuedOrder {
	/// Parachain ID
	pub para_id: ParaId,
}

pub trait ThresholdParameterT:
	Parameter + Member + Default + MaybeSerializeDeserialize + MaxEncodedLen
{
}

impl<T> ThresholdParameterT for T where
	T: Parameter + Member + Default + MaybeSerializeDeserialize + MaxEncodedLen
{
}

sp_api::decl_runtime_apis! {
	#[api_version(2)]
	pub trait OnDemandRuntimeApi<Balance, BlockNumber, ThresholdParameter> where
		Balance: Codec + MaybeDisplay,
		BlockNumber: Codec + From<u32>,
		ThresholdParameter: ThresholdParameterT,
	{
		/// Order placement slot width.
		fn slot_width()-> BlockNumber;

		/// Runtime configured order placement threshold parameter.
		fn threshold_parameter() -> ThresholdParameter;
	}
}

#[derive(Encode, Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct OrderInherentData<AuthorityPub: Send + Sync + Codec> {
	pub relay_storage_proof: sp_trie::StorageProof,
	pub para_id: ParaId,
	pub author_pub: AuthorityPub,
}

impl<AuthorityPub: Send + Sync + Codec> OrderInherentData<AuthorityPub> {
	pub async fn create_at(
		relay_chain_interface: &impl RelayChainInterface,
		para_id: ParaId,
		author_pub: AuthorityPub,
	) -> Option<Self> {
		let best_hash = relay_chain_interface.best_block_hash().await.unwrap(); // TODO
		let header = relay_chain_interface.header(BlockId::Hash(best_hash)).await.unwrap().unwrap(); // TODO

		let relay_storage_proof =
			collect_relay_storage_proof(relay_chain_interface, header.hash()).await?;

		// let relay_storage_proof = RelayChainStateProof::new(para_id, header.state_root, proof);

		Some(OrderInherentData {
			relay_storage_proof: relay_storage_proof.clone(),
			para_id,
			author_pub,
		})
	}
}

#[async_trait::async_trait]
impl<AuthorityPub: Send + Sync + Codec> sp_inherents::InherentDataProvider
	for OrderInherentData<AuthorityPub>
{
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		inherent_data.put_data(ON_DEMAND_INHERENT_IDENTIFIER, &self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

async fn collect_relay_storage_proof(
	relay_chain: &impl RelayChainInterface,
	relay_parent: PHash,
) -> Option<sp_state_machine::StorageProof> {
	let mut relevant_keys = Vec::new();
	// Get storage proof for events at a specific block.
	relevant_keys.push(EVENTS.to_vec());

	let relay_storage_proof = relay_chain.prove_read(relay_parent, &relevant_keys).await;
	match relay_storage_proof {
		Ok(proof) => Some(proof),
		Err(err) => {
			log::info!("RelayChainError:{:?}", err);
			None
		},
	}
}
