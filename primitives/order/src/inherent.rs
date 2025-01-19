use crate::{OrderInherentData, EVENTS, ON_DEMAND_INHERENT_IDENTIFIER};
use cumulus_primitives_core::relay_chain::BlockId;
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::PHash;
use cumulus_relay_chain_interface::RelayChainInterface;

const LOG_TARGET: &str = "order-inherent";

impl OrderInherentData {
    pub async fn create_at(
        relay_chain_interface: &impl RelayChainInterface,
        para_id: ParaId,
    ) -> Option<Self> {
        let best_hash = relay_chain_interface
            .best_block_hash()
            .await
            .map_err(|e| {
                log::error!(
                    target: LOG_TARGET,
                    "Failed to get best relay chain block hash. Error: {:?}",
                    e
                );
            })
            .ok()?;

        let header = relay_chain_interface
            .header(BlockId::Hash(best_hash))
            .await
            .map_err(|e| {
                log::error!(
                    target: LOG_TARGET,
                    "Failed to fetch relay chain header: {}, Error: {:?}",
                    best_hash,
                    e
                );
            })
            .ok()??;

        let relay_storage_proof =
            collect_relay_storage_proof(relay_chain_interface, header.hash()).await?;

        log::info!(
            target: LOG_TARGET,
            "Submitting inherent data"
        );

        Some(OrderInherentData {
            relay_storage_proof: relay_storage_proof.clone(),
            para_id,
        })
    }
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for OrderInherentData {
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
        }
    }
}
