use byteorder::{ByteOrder, LittleEndian};
use libmdbx::*;
use tempfile::tempdir;

type Database = libmdbx::Database<NoWriteMap>;

#[test]
fn test_open() {
    let dir = tempdir().unwrap();

    // opening non-existent database with read-only should fail
    assert!(Database::new()
        .set_flags(Mode::ReadOnly.into())
        .open(dir.path())
        .is_err());

    // opening non-existent database should succeed
    assert!(Database::new().open(dir.path()).is_ok());

    // opening database with read-only should succeed
    assert!(Database::new()
        .set_flags(Mode::ReadOnly.into())
        .open(dir.path())
        .is_ok());
}

#[test]
fn test_begin_txn() {
    let dir = tempdir().unwrap();

    {
        // writable database
        let db = Database::new().open(dir.path()).unwrap();

        assert!(db.begin_rw_txn().is_ok());
        assert!(db.begin_ro_txn().is_ok());
    }

    {
        // read-only database
        let db = Database::new()
            .set_flags(Mode::ReadOnly.into())
            .open(dir.path())
            .unwrap();

        assert!(db.begin_rw_txn().is_err());
        assert!(db.begin_ro_txn().is_ok());
    }
}

#[test]
fn test_open_table() {
    let dir = tempdir().unwrap();
    let db = Database::new().set_max_tables(1).open(dir.path()).unwrap();

    let txn = db.begin_ro_txn().unwrap();
    assert!(txn.open_table(None).is_ok());
    assert!(txn.open_table(Some("test")).is_err());
}

#[test]
fn test_create_table() {
    let dir = tempdir().unwrap();
    let db = Database::new().set_max_tables(11).open(dir.path()).unwrap();

    let txn = db.begin_rw_txn().unwrap();
    assert!(txn.open_table(Some("test")).is_err());
    assert!(txn.create_table(Some("test"), TableFlags::empty()).is_ok());
    assert!(txn.open_table(Some("test")).is_ok())
}

#[test]
fn test_close_table() {
    let dir = tempdir().unwrap();
    let db = Database::new().set_max_tables(10).open(dir.path()).unwrap();

    let txn = db.begin_rw_txn().unwrap();
    txn.create_table(Some("test"), TableFlags::empty()).unwrap();
    txn.open_table(Some("test")).unwrap();
}

#[test]
fn test_sync() {
    let dir = tempdir().unwrap();
    {
        let db = Database::new().open(dir.path()).unwrap();
        db.sync(true).unwrap();
    }
    {
        let db = Database::new()
            .set_flags(Mode::ReadOnly.into())
            .open(dir.path())
            .unwrap();
        db.sync(true).unwrap_err();
    }
}

#[test]
fn test_stat() {
    let dir = tempdir().unwrap();
    let db = Database::new().open(dir.path()).unwrap();

    // Stats should be empty initially.
    let stat = db.stat().unwrap();
    assert_eq!(stat.depth(), 0);
    assert_eq!(stat.branch_pages(), 0);
    assert_eq!(stat.leaf_pages(), 0);
    assert_eq!(stat.overflow_pages(), 0);
    assert_eq!(stat.entries(), 0);

    // Write a few small values.
    for i in 0..64 {
        let mut value = [0u8; 8];
        LittleEndian::write_u64(&mut value, i);
        let tx = db.begin_rw_txn().unwrap();
        tx.put(
            &tx.open_table(None).unwrap(),
            value,
            value,
            WriteFlags::default(),
        )
        .unwrap();
        tx.commit().unwrap();
    }

    // Stats should now reflect inserted values.
    let stat = db.stat().unwrap();
    assert_eq!(stat.depth(), 1);
    assert_eq!(stat.branch_pages(), 0);
    assert_eq!(stat.leaf_pages(), 1);
    assert_eq!(stat.overflow_pages(), 0);
    assert_eq!(stat.entries(), 64);
}

#[test]
fn test_info() {
    let map_size = 1024 * 1024;
    let dir = tempdir().unwrap();
    let db = Database::new()
        .set_geometry(Geometry {
            size: Some(map_size..),
            ..Default::default()
        })
        .open(dir.path())
        .unwrap();

    let info = db.info().unwrap();
    assert_eq!(info.geometry().min(), map_size as u64);
    // assert_eq!(info.last_pgno(), 1);
    // assert_eq!(info.last_txnid(), 0);
    assert_eq!(info.num_readers(), 0);
}

#[test]
fn test_freelist() {
    let dir = tempdir().unwrap();
    let db = Database::new().open(dir.path()).unwrap();

    let mut freelist = db.freelist().unwrap();
    assert_eq!(freelist, 0);

    // Write a few small values.
    for i in 0..64 {
        let mut value = [0u8; 8];
        LittleEndian::write_u64(&mut value, i);
        let tx = db.begin_rw_txn().unwrap();
        tx.put(
            &tx.open_table(None).unwrap(),
            value,
            value,
            WriteFlags::default(),
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let tx = db.begin_rw_txn().unwrap();
    tx.clear_table(&tx.open_table(None).unwrap()).unwrap();
    tx.commit().unwrap();

    // Freelist should not be empty after clear_table.
    freelist = db.freelist().unwrap();
    assert!(freelist > 0);
}
