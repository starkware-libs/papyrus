use std::sync::Arc;

use pretty_assertions::assert_eq;
use starknet_client::reader::{MockStarknetReader, PendingData};

use crate::sources::pending::{GenericPendingSource, PendingSourceTrait};

#[tokio::test]
async fn get_pending_data() {
    let mut client_mock = MockStarknetReader::new();

    // We need to perform all the mocks before moving the mock into pending_source.
    // TODO(dvir): use pending_data which isn't the default.
    client_mock.expect_pending_data().times(1).returning(|| Ok(Some(PendingData::default())));

    let pending_source = GenericPendingSource { starknet_client: Arc::new(client_mock) };

    let pending_data = pending_source.get_pending_data().await.unwrap();
    assert_eq!(pending_data, PendingData::default());
}
