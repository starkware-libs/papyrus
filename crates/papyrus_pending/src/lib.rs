use starknet_client::reader::{PendingBlock, PendingData};

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
