use tempfile::tempdir;

use crate::starknet::BlockNumber;
use crate::storage::create_storage_components;

use super::components::StorageComponents;

#[tokio::test]
async fn test_add_block_number() {
    let dir = tempdir().unwrap();
    let StorageComponents {
        info_reader,
        mut info_writer,
    } = create_storage_components(dir.path()).await.unwrap();
    let expected = BlockNumber(5);

    info_writer.set_latest_block_number(expected).await.unwrap();

    let res = info_reader.get_latest_block_number().await;
    assert_eq!(res.unwrap(), expected);
}
