//! Stock, pure Aura collators.
//!
//! This includes the [`basic`] collator.

use crate::collator::SlotClaim;
use codec::Codec;
use cumulus_client_consensus_common::{
	self as consensus_common, load_abridged_host_configuration, ParentSearchParams,
};
use cumulus_primitives_aura::{AuraUnincludedSegmentApi, Slot};
use cumulus_primitives_core::{relay_chain::Hash as ParaHash, BlockT};
use cumulus_relay_chain_interface::RelayChainInterface;
use polkadot_node_subsystem_util::runtime::ClaimQueueSnapshot;
use polkadot_primitives::{
	AsyncBackingParams, CoreIndex, Hash as RelayHash, Id as ParaId, OccupiedCoreAssumption,
	ValidationCodeHash,
};
use sc_consensus_aura::{standalone as aura_internal, AuraApi};
use sp_api::ProvideRuntimeApi;
use sp_core::Pair;
use sp_keystore::KeystorePtr;
use sp_timestamp::Timestamp;

pub mod basic;

/// Check the `local_validation_code_hash` against the validation code hash in the relay chain
/// state.
///
/// If the code hashes do not match, it prints a warning.
async fn check_validation_code_or_log(
	local_validation_code_hash: &ValidationCodeHash,
	para_id: ParaId,
	relay_client: &impl RelayChainInterface,
	relay_parent: RelayHash,
) {
	let state_validation_code_hash = match relay_client
		.validation_code_hash(relay_parent, para_id, OccupiedCoreAssumption::Included)
		.await
	{
		Ok(hash) => hash,
		Err(error) => {
			tracing::debug!(
				target: super::LOG_TARGET,
				%error,
				?relay_parent,
				%para_id,
				"Failed to fetch validation code hash",
			);
			return
		},
	};

	match state_validation_code_hash {
		Some(state) =>
			if state != *local_validation_code_hash {
				tracing::warn!(
					target: super::LOG_TARGET,
					%para_id,
					?relay_parent,
					?local_validation_code_hash,
					relay_validation_code_hash = ?state,
					"Parachain code doesn't match validation code stored in the relay chain state.",
				);
			},
		None => {
			tracing::warn!(
				target: super::LOG_TARGET,
				%para_id,
				?relay_parent,
				"Could not find validation code for parachain in the relay chain state.",
			);
		},
	}
}
