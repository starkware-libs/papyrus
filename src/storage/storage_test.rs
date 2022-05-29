use crate::{
    starknet::BlockNumber,
    storage::{create_store_access, StorageReader, StorageWriter},
};

#[tokio::test]
async fn test_add_block_number() {
    //we use unwrap throughout this function since it's
    //a test function using an internal mock implementation.

    let (reader, mut writer) = create_store_access().unwrap();
    let expected = BlockNumber(5);

    writer.set_latest_block_number(expected).await.unwrap();

    let res = reader.get_latest_block_number().await;
    assert_eq!(res.unwrap(), expected);
}
