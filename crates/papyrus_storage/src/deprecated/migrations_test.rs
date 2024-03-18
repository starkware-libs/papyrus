use std::fmt::Debug;

use starknet_api::block::{BlockNumber, StarknetVersion};
use starknet_api::core::{EventCommitment, TransactionCommitment};
use starknet_api::hash::StarkFelt;
use starknet_api::stark_felt;
use test_log::test;
use test_utils::{get_rng, GetTestInstance};

use crate::db::serialization::VersionZeroWrapper;
use crate::db::table_types::Table;
use crate::deprecated::migrations::{StorageBlockHeaderV0, StorageBlockHeaderV1};
use crate::header::{HeaderStorageReader, HeaderStorageWriter};
use crate::test_utils::get_test_storage;
use crate::{MarkerKind, StorageReader, StorageWriter, ValueSerde, VersionWrapper};

fn write_old_headers<TVersionWrapper: ValueSerde + Debug>(
    writer: &mut StorageWriter,
    reader: &StorageReader,
    headers: &[&TVersionWrapper::Value],
) {
    // No easy way to insert a deprecated version into the db.
    let table_id = writer
        .db_writer
        .create_simple_table::<BlockNumber, TVersionWrapper>(reader.tables.headers.name)
        .unwrap();
    let txn = writer.begin_rw_txn().unwrap();
    let table = txn.open_table(&table_id).unwrap();
    for (i, header) in headers.iter().enumerate() {
        table.insert(&txn.txn, &BlockNumber(i.try_into().unwrap()), header).unwrap();
    }
    txn.open_table(&txn.tables.markers)
        .unwrap()
        .upsert(&txn.txn, &MarkerKind::Header, &BlockNumber(headers.len().try_into().unwrap()))
        .unwrap();
    txn.commit().unwrap();
}

#[test]
fn header_v0_to_current() {
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

    write_old_headers::<VersionZeroWrapper<StorageBlockHeaderV0>>(
        &mut writer,
        &reader,
        &[&header_without_commitments, &header_with_commitments],
    );

    // Read the headers, expecting to get the latest version via the migration.
    let header_current_no_commitments =
        reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
    assert!(header_current_no_commitments.is_some());
    let header_current_no_commitments = header_current_no_commitments.unwrap();
    assert!(header_current_no_commitments.state_diff_commitment.is_none());
    assert!(header_current_no_commitments.event_commitment.is_none());
    assert!(header_current_no_commitments.n_transactions.is_none());
    assert!(header_current_no_commitments.n_events.is_none());

    let header_current_with_commitments =
        reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(1)).unwrap();
    assert!(header_current_with_commitments.is_some());
    let header_current_with_commitments = header_current_with_commitments.unwrap();
    // In V0 there is no state_diff_commitment.
    assert!(header_current_with_commitments.state_diff_commitment.is_none());
    assert_eq!(
        header_current_with_commitments.transaction_commitment,
        Some(TransactionCommitment(stark_felt!("0x1")))
    );
    assert_eq!(
        header_current_with_commitments.event_commitment,
        Some(EventCommitment(stark_felt!("0x2")))
    );
    assert_eq!(header_current_with_commitments.n_transactions, Some(3));
    assert_eq!(header_current_with_commitments.n_events, Some(4));
}

#[test]
fn header_v1_to_current() {
    let ((reader, mut writer), _dir) = get_test_storage();
    // Insert a headers V1 to the db.
    let header_v1 = StorageBlockHeaderV1::get_test_instance(&mut get_rng());
    writer
        .begin_rw_txn()
        .unwrap()
        .update_starknet_version(&BlockNumber(0), &StarknetVersion::default())
        .unwrap()
        .commit()
        .unwrap();

    write_old_headers::<VersionWrapper<StorageBlockHeaderV1, 1>>(
        &mut writer,
        &reader,
        &[&header_v1],
    );

    // Read the headers, expecting to get the V1 version via the migration.
    let header_current = reader.begin_ro_txn().unwrap().get_block_header(BlockNumber(0)).unwrap();
    let header_current = header_current.unwrap();
    assert!(header_current.receipt_commitment.is_none());
    assert!(header_current.state_diff_length.is_none());
    assert_eq!(header_current.block_hash, header_v1.block_hash);
    assert_eq!(header_current.parent_hash, header_v1.parent_hash);
    assert_eq!(header_current.block_number, header_v1.block_number);
    assert_eq!(header_current.l1_gas_price, header_v1.l1_gas_price);
    assert_eq!(header_current.l1_data_gas_price, header_v1.l1_data_gas_price);
    assert_eq!(header_current.state_root, header_v1.state_root);
    assert_eq!(header_current.sequencer, header_v1.sequencer);
    assert_eq!(header_current.timestamp, header_v1.timestamp);
    assert_eq!(header_current.l1_da_mode, header_v1.l1_da_mode);
    assert_eq!(header_current.state_diff_commitment, header_v1.state_diff_commitment);
    assert_eq!(header_current.transaction_commitment, header_v1.transaction_commitment);
    assert_eq!(header_current.event_commitment, header_v1.event_commitment);
    assert_eq!(header_current.n_transactions, header_v1.n_transactions);
    assert_eq!(header_current.n_events, header_v1.n_events);
}
