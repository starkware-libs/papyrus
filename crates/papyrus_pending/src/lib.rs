// TODO(shahak): remove allow(dead_code).
#![allow(dead_code)]
#[cfg(test)]
mod pending_test;

use starknet_client::reader::{PendingBlock, PendingData, ReaderClientError, StarknetReader};

pub struct GenericPendingSync<TStarknetClient: StarknetReader + Send + Sync> {
    starknet_client: TStarknetClient,
    pending_state: Option<PendingData>,
}

// TODO(shahak): Return the class hashes of the new transactions
fn should_swap(old_data: &Option<PendingData>, new_data: &Option<PendingData>) -> bool {
    match (old_data, new_data) {
        (None, None) | (Some(_), None) => false,
        (None, Some(_)) => true,
        (
            Some(PendingData {
                block: PendingBlock { transactions: old_transactions, .. }, ..
            }),
            Some(PendingData {
                block: PendingBlock { transactions: new_transactions, .. }, ..
            }),
            // TODO(shahak): Decide what to do if old_transactions and new_transactions don't share
            // the same prefix
        ) => old_transactions.len() < new_transactions.len(),
    }
}

impl<TStarknetClient: StarknetReader + Send + Sync> GenericPendingSync<TStarknetClient> {
    async fn update_pending(&mut self) -> Result<(), ReaderClientError> {
        let new_pending = self.starknet_client.pending_data().await?;
        if should_swap(&self.pending_state, &new_pending) {
            self.pending_state = new_pending;
        }
        Ok(())
    }

    fn new(starknet_client: TStarknetClient) -> Self {
        Self { starknet_client, pending_state: None }
    }
}
