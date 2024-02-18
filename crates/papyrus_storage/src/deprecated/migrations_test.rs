use starknet_api::block::{BlockNumber, StarknetVersion};
use starknet_api::core::{EventCommitment, TransactionCommitment};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use test_log::test;

use crate::db::serialization::VersionZeroWrapper;
use crate::db::table_types::Table;
use crate::deprecated::migrations::StorageBlockHeaderV0;
use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_storage;
use crate::MarkerKind;

#[test]
fn header_v0_to_v1() {
    let ((reader, mut writer), _dir) = get_test_storage();
    // Insert a headers V0 to the db.
    let header_without_commitments = StorageBlockHeaderV0::default();
    let header_with_commitments = StorageBlockHeaderV0 {
        transaction_commitment: TransactionCommitment(stark_felt!("0x1")),
        event_commitment: EventCommitment(stark_felt!("0x2")),
        n_transactions: 3,
        n_events: 4,
        ..StorageBlockHeaderV0::default()
    };
    writer
        .begin_rw_txn()
        .unwrap()
        .update_starknet_version(&BlockNumber(0), &StarknetVersion::default())
        .unwrap()
        .commit()
        .unwrap();

    // No easy way to insert a deprecated version into the db.
    let v0_table_id = writer
        .db_writer
        .create_simple_table::<BlockNumber, VersionZeroWrapper<StorageBlockHeaderV0>>(
            reader.tables.headers.name,
        )
        .unwrap();
    let txn = writer.begin_rw_txn().unwrap();
    let v0_table = txn.open_table(&v0_table_id).unwrap();
    v0_table.insert(&txn.txn, &BlockNumber(0), &header_without_commitments).unwrap();
    v0_table.insert(&txn.txn, &BlockNumber(1), &header_with_commitments).unwrap();
    txn.open_table(&txn.tables.markers)
        .unwrap()
        .upsert(&txn.txn, &MarkerKind::Header, &BlockNumber(2))
        .unwrap();
    txn.commit().unwrap();

    // Read the headers, expecting to get the V1 version via the migration.
    let header_v1_no_commitments =
        reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
    assert!(header_v1_no_commitments.is_some());
    let header_v1_no_commitments = header_v1_no_commitments.unwrap();
    assert!(header_v1_no_commitments.state_diff_commitment.is_none());
    assert!(header_v1_no_commitments.event_commitment.is_none());
    assert!(header_v1_no_commitments.n_transactions.is_none());
    assert!(header_v1_no_commitments.n_events.is_none());

    let header_v1_with_commitments =
        reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap();
    assert!(header_v1_with_commitments.is_some());
    let header_v1_with_commitments = header_v1_with_commitments.unwrap();
    // In V0 there is no state_diff_commitment.
    assert!(header_v1_with_commitments.state_diff_commitment.is_none());
    assert_eq!(
        header_v1_with_commitments.transaction_commitment,
        Some(TransactionCommitment(stark_felt!("0x1")))
    );
    assert_eq!(
        header_v1_with_commitments.event_commitment,
        Some(EventCommitment(stark_felt!("0x2")))
    );
    assert_eq!(header_v1_with_commitments.n_transactions, Some(3));
    assert_eq!(header_v1_with_commitments.n_events, Some(4));
}
