use crate::db::{open_env, DbReader, DbWriter};
use crate::test_utils::get_test_config;

fn get_test_env() -> Result<(DbReader, DbWriter), anyhow::Error> {
    let config = get_test_config()?;
    Ok(open_env(config)?)
}

#[test]
fn open_env_scenario() -> Result<(), anyhow::Error> {
    get_test_env()?;
    Ok(())
}

#[test]
fn txns_scenarios() -> Result<(), anyhow::Error> {
    // Create an environment and a table.
    let (reader, mut writer) = get_test_env()?;
    let table_id = writer.create_table::<[u8; 3], [u8; 5]>("table")?;

    // Snapshot state by creating a read txn.
    let txn0 = reader.begin_ro_txn()?;
    let table = txn0.open_table(&table_id)?;

    // Insert a value.
    let wtxn = writer.begin_rw_txn()?;
    table.insert(&wtxn, b"key", b"data0")?;
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn1 = reader.begin_ro_txn()?;

    // Update the value.
    let wtxn = writer.begin_rw_txn()?;
    table.upsert(&wtxn, b"key", b"data1")?;
    wtxn.commit().unwrap();

    // Snapshot state by creating a read txn.
    let txn2 = reader.begin_ro_txn()?;

    // Delete the value.
    let wtxn2 = writer.begin_rw_txn()?;
    table.delete(&wtxn2, b"key")?;
    wtxn2.commit()?;

    // Snapshot state by creating a read txn.
    let txn3 = reader.begin_ro_txn()?;

    // Validate data in snapshots.
    assert_eq!(table.get(&txn0, b"key")?, None);
    assert_eq!(table.get(&txn1, b"key")?, Some(*b"data0"));
    assert_eq!(table.get(&txn2, b"key")?, Some(*b"data1"));
    assert_eq!(table.get(&txn3, b"key")?, None);

    Ok(())
}

#[test]
fn table_stats() -> Result<(), anyhow::Error> {
    // Create an environment and a table.
    let (reader, mut writer) = get_test_env()?;
    let table_id = writer.create_table::<[u8; 3], [u8; 5]>("table")?;

    // Empty table stats.
    let empty_stat = reader.get_table_stats("table")?;
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 0);
    assert_eq!(empty_stat.entries, 0);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 0);

    // Insert a value.
    let wtxn = writer.begin_rw_txn()?;
    let table = wtxn.open_table(&table_id)?;
    table.insert(&wtxn, b"key", b"data0")?;
    wtxn.commit()?;

    // Non-empty table stats.
    let empty_stat = reader.get_table_stats("table")?;
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 1);
    assert_eq!(empty_stat.entries, 1);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 1);

    // Delete the value.
    let wtxn = writer.begin_rw_txn()?;
    let table = wtxn.open_table(&table_id)?;
    table.delete(&wtxn, b"key")?;
    wtxn.commit()?;

    // Empty table stats.
    let empty_stat = reader.get_table_stats("table")?;
    assert_eq!(empty_stat.branch_pages, 0);
    assert_eq!(empty_stat.depth, 0);
    assert_eq!(empty_stat.entries, 0);
    assert_eq!(empty_stat.overflow_pages, 0);
    assert_eq!(empty_stat.leaf_pages, 0);

    Ok(())
}
