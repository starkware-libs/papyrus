use std::sync::Arc;

use assert_matches::assert_matches;
use papyrus_common::{BlockHashAndNumber, SyncingState};
use starknet_api::block::BlockNumber;
use tokio::sync::RwLock;

use crate::syncing_state::update_syncing_state;

#[tokio::test]
async fn test_update_syncing_state() {
    // Starting = 3, Current = 3, Highest = 10 => syncing.
    let shared_syncing_state = Arc::new(RwLock::new(SyncingState::Synced));
    let highest_block_num = BlockNumber(10);
    let starting_block_num = BlockNumber(3);
    update_syncing_state(
        shared_syncing_state.clone(),
        BlockHashAndNumber { block_number: starting_block_num, ..Default::default() },
        BlockHashAndNumber { block_number: highest_block_num, ..Default::default() },
    )
    .await;
    let syncing_state = *shared_syncing_state.read().await;
    assert_matches!(
        syncing_state,
        SyncingState::SyncStatus(sync_status)
        if (
            sync_status.starting_block_num == starting_block_num
            && sync_status.current_block_num == starting_block_num
            && sync_status.highest_block_num == highest_block_num
        )
    );

    // Starting = 3, Current = 8, Highest = 10 => syncing.
    let current_block_num = BlockNumber(8);
    update_syncing_state(
        shared_syncing_state.clone(),
        BlockHashAndNumber { block_number: current_block_num, ..Default::default() },
        BlockHashAndNumber { block_number: highest_block_num, ..Default::default() },
    )
    .await;
    let syncing_state = *shared_syncing_state.read().await;
    assert_matches!(
        syncing_state,
        SyncingState::SyncStatus(sync_status)
        if (
            sync_status.starting_block_num == starting_block_num
            && sync_status.current_block_num == current_block_num
            && sync_status.highest_block_num == highest_block_num
        )
    );

    // Starting = 3, Current = 10, Highest = 10 => synced.
    update_syncing_state(
        shared_syncing_state.clone(),
        BlockHashAndNumber { block_number: highest_block_num, ..Default::default() },
        BlockHashAndNumber { block_number: highest_block_num, ..Default::default() },
    )
    .await;
    let syncing_state = *shared_syncing_state.read().await;
    assert_matches!(syncing_state, SyncingState::Synced);

    // Starting = 10, Current = 10, Highest = 11 => syncing.
    let next_highest_block_num = BlockNumber(11);
    update_syncing_state(
        shared_syncing_state.clone(),
        BlockHashAndNumber { block_number: highest_block_num, ..Default::default() },
        BlockHashAndNumber { block_number: next_highest_block_num, ..Default::default() },
    )
    .await;
    let syncing_state = *shared_syncing_state.read().await;
    assert_matches!(
        syncing_state,
        SyncingState::SyncStatus(sync_status)
        if (
            sync_status.starting_block_num == highest_block_num
            && sync_status.current_block_num == highest_block_num
            && sync_status.highest_block_num == next_highest_block_num
        )
    );
}
