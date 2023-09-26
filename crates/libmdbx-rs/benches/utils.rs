use libmdbx::{Database, NoWriteMap, WriteFlags};
use tempfile::{tempdir, TempDir};

pub fn get_key(n: u32) -> String {
    format!("key{n}")
}

pub fn get_data(n: u32) -> String {
    format!("data{n}")
}

pub fn setup_bench_db(num_rows: u32) -> (TempDir, Database<NoWriteMap>) {
    let dir = tempdir().unwrap();
    let db = Database::new().open(dir.path()).unwrap();

    {
        let txn = db.begin_rw_txn().unwrap();
        let table = txn.open_table(None).unwrap();
        for i in 0..num_rows {
            txn.put(&table, get_key(i), get_data(i), WriteFlags::empty())
                .unwrap();
        }
        txn.commit().unwrap();
    }
    (dir, db)
}
