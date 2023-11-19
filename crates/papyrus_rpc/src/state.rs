use papyrus_execution::objects::PendingStateDiff;
use starknet_client::reader::objects::state::StateDiff;

pub(crate) fn client_state_diff_to_execution_state_diff(
    client_state_diff: StateDiff,
) -> PendingStateDiff {
    PendingStateDiff {
        storage_diffs: client_state_diff.storage_diffs,
        deployed_contracts: client_state_diff.deployed_contracts,
        declared_classes: client_state_diff.declared_classes,
        old_declared_contracts: client_state_diff.old_declared_contracts,
        nonces: client_state_diff.nonces,
        replaced_classes: client_state_diff.replaced_classes,
    }
}
