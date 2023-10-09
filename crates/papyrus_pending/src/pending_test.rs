use pretty_assertions::assert_eq;
use starknet_client::reader::objects::transaction::Transaction;
use starknet_client::reader::{MockStarknetReader, PendingBlock, PendingData};
use test_utils::{get_rng, GetTestInstance};

use super::GenericPendingSync;

fn pending_data_with_transactions(transactions: Vec<Transaction>) -> PendingData {
    PendingData { block: PendingBlock { transactions, ..Default::default() }, ..Default::default() }
}

#[tokio::test]
async fn update_pending() {
    let mut rng = get_rng();
    let transaction1 = Transaction::get_test_instance(&mut rng);
    let transaction2 = Transaction::get_test_instance(&mut rng);
    let transaction3 = Transaction::get_test_instance(&mut rng);
    let pending_datas_and_should_update = vec![
        (None, true),
        (
            Some(pending_data_with_transactions(vec![transaction1.clone(), transaction2.clone()])),
            true,
        ),
        (Some(pending_data_with_transactions(vec![transaction1.clone()])), false),
        (
            Some(pending_data_with_transactions(vec![transaction1, transaction2, transaction3])),
            true,
        ),
        (None, false),
    ];
    let mut mock = MockStarknetReader::new();
    for (pending_data_from_starknet, _) in pending_datas_and_should_update.iter().cloned() {
        mock.expect_pending_data()
            .times(1)
            .returning(move || Ok(pending_data_from_starknet.clone()));
    }

    let mut pending_sync = GenericPendingSync::new(mock);

    let mut expected_data = None;
    for (pending_data, should_update) in pending_datas_and_should_update {
        if should_update {
            expected_data = pending_data;
        }
        pending_sync.update_pending().await.unwrap();
        assert_eq!(expected_data, pending_sync.pending_state);
    }
}
