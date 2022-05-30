use std::sync::Arc;

use libmdbx::{DatabaseFlags, TableObject, WriteFlags};
use tokio::sync::Mutex;

use crate::{
    starknet::BlockNumber,
    storage::db::{DbReader, DbWriter, StorageError},
};

// Constants.
const DB_INFO: Option<&'static str> = Some("info");

pub struct InfoReader {
    db_reader: DbReader,
}

pub struct InfoWriter {
    db_writer_mutex: Arc<Mutex<DbWriter>>,
}

pub fn info_prepare(db_writer: &mut DbWriter) -> Result<(), libmdbx::Error> {
    let tx = db_writer.begin_rw_txn()?;
    tx.create_db(DB_INFO, DatabaseFlags::empty())?;
    tx.commit()?;
    Ok(())
}

impl InfoReader {
    pub fn new(db_reader: DbReader) -> InfoReader {
        InfoReader { db_reader }
    }
    pub async fn get_latest_block_number(&self) -> Result<BlockNumber, StorageError> {
        let tx = self.db_reader.begin_ro_txn()?;
        let db = tx.open_db(DB_INFO)?;
        let res = tx.get::<BlockNumber>(&db, b"block_number")?;
        Ok(res.unwrap_or(BlockNumber(0)))
    }
}

impl InfoWriter {
    pub fn new(db_writer_mutex: Arc<Mutex<DbWriter>>) -> InfoWriter {
        InfoWriter { db_writer_mutex }
    }
    pub async fn set_latest_block_number(&mut self, n: BlockNumber) -> Result<(), StorageError> {
        let db_writer = self.db_writer_mutex.lock().await;
        let tx = db_writer.begin_rw_txn()?;
        let db = tx.open_db(DB_INFO)?;
        tx.put(
            &db,
            b"block_number",
            bincode::serialize(&n).unwrap(),
            WriteFlags::empty(),
        )?;
        tx.commit()?;
        Ok(())
    }
}

impl<'tx> TableObject<'tx> for BlockNumber {
    fn decode(data_val: &[u8]) -> Result<Self, libmdbx::Error>
    where
        Self: Sized,
    {
        Ok(bincode::deserialize(data_val).expect("Bad db serialization"))
    }
}
