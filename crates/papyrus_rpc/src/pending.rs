use papyrus_common::pending_classes::PendingClasses;
use papyrus_execution::objects::PendingData as ExecutionPendingData;
use starknet_client::reader::objects::pending_data::PendingData as ClientPendingData;

pub(crate) fn client_pending_data_to_execution_pending_data(
    client_pending_data: ClientPendingData,
    pending_classes: PendingClasses,
) -> ExecutionPendingData {
    ExecutionPendingData {
        storage_diffs: client_pending_data.state_update.state_diff.storage_diffs,
        deployed_contracts: client_pending_data.state_update.state_diff.deployed_contracts,
        declared_classes: client_pending_data.state_update.state_diff.declared_classes,
        old_declared_contracts: client_pending_data.state_update.state_diff.old_declared_contracts,
        nonces: client_pending_data.state_update.state_diff.nonces,
        replaced_classes: client_pending_data.state_update.state_diff.replaced_classes,
        classes: pending_classes,
        timestamp: client_pending_data.block.timestamp(),
        eth_l1_gas_price: client_pending_data.block.l1_gas_price().price_in_wei,
        strk_l1_gas_price: client_pending_data.block.l1_gas_price().price_in_fri,
        sequencer: client_pending_data.block.sequencer_address(),
    }
}
