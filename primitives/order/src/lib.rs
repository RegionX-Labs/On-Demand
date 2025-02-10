#![cfg_attr(not(feature = "std"), no_std)]

use crate::well_known_keys::EVENTS;
use codec::{Codec, Decode, Encode, MaxEncodedLen};
use cumulus_primitives_core::{relay_chain::BlockNumber as RelayBlockNumber, ParaId};
use frame_support::{pallet_prelude::InherentIdentifier, Parameter};
use scale_info::TypeInfo;
use sp_core::H256;
use sp_runtime::traits::{MaybeDisplay, MaybeSerializeDeserialize, Member};

#[cfg(feature = "std")]
pub mod inherent;
pub mod well_known_keys;

// Identifier of the order inherent
pub const ON_DEMAND_INHERENT_IDENTIFIER: InherentIdentifier = *b"orderiht";

#[derive(Encode, Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct OrderInherentData {
	pub data: Option<InherentData>,
}

#[derive(Encode, Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct InherentData {
	pub relay_storage_proof: sp_trie::StorageProof,
	pub relay_state_root: H256,
	pub relay_height: RelayBlockNumber,
	pub para_id: ParaId,
}

#[derive(Encode, Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct OrderRecord {
	/// The hash of the block in which the order was placed.
	pub relay_block_hash: Option<H256>,
}

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
