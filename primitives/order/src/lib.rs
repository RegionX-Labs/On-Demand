#![cfg_attr(not(feature = "std"), no_std)]

use crate::well_known_keys::EVENTS;
use codec::{Decode, Encode, MaxEncodedLen, Codec};
use cumulus_primitives_core::ParaId;
use frame_support::{pallet_prelude::InherentIdentifier, Parameter};
use sp_runtime::traits::{MaybeDisplay, MaybeSerializeDeserialize, Member};
use scale_info::TypeInfo;

#[cfg(feature = "std")]
pub mod inherent;
pub mod well_known_keys;

// Identifier of the order inherent
pub const ON_DEMAND_INHERENT_IDENTIFIER: InherentIdentifier = *b"orderiht";

#[derive(Encode, Decode, sp_core::RuntimeDebug, Clone, PartialEq, TypeInfo)]
pub struct OrderInherentData {
    pub relay_storage_proof: sp_trie::StorageProof,
    pub para_id: ParaId,
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
