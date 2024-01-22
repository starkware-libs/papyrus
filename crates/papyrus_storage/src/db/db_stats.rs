use human_bytes::human_bytes;
use libmdbx::Info;
use serde::{Deserialize, Serialize};

use super::{DbReader, DbResult};

/// A single table statistics.
#[derive(Serialize, Deserialize, Debug)]
pub struct DbTableStats {
    /// Number of entries in the table.
    pub entries: usize,
    /// Number of branch pages in the table.
    pub branch_pages: usize,
    /// Depth of the table.
    pub depth: u32,
    /// Number of leaf pages in the table.
    pub leaf_pages: usize,
    /// Number of overflow pages in the table.
    pub overflow_pages: usize,
    /// Total size of the table.
    #[serde(serialize_with = "readable_bytes")]
    pub total_size: u64,
    /// The table size as a portion of the whole database size.
    #[serde(serialize_with = "float_precision")]
    pub db_portion: f64,
}

#[derive(Serialize, Deserialize, Debug)]
/// Statics about the whole database.
pub struct DbWholeStats {
    /// Number of entries in the database.
    pub entries: usize,
    /// Number of branch pages in the database.
    pub branch_pages: usize,
    /// Number of leaf pages in the database.
    pub leaf_pages: usize,
    /// Number of overflow pages in the database.
    pub overflow_pages: usize,
    /// Total size of the database.
    #[serde(serialize_with = "readable_bytes")]
    pub total_size: u64,
    /// Page size of the database.
    #[serde(serialize_with = "readable_bytes")]
    pub page_size: u64,
    /// The number of pages in the free list.
    pub freelist_size: usize,
}

impl DbReader {
    // Returns statistics about a specific table in the database.
    pub(crate) fn get_table_stats(&self, name: &str) -> DbResult<DbTableStats> {
        let db_txn = self.begin_ro_txn()?;
        let table = db_txn.txn.open_table(Some(name))?;
        let stat = db_txn.txn.table_stat(&table)?;
        Ok(DbTableStats {
            branch_pages: stat.branch_pages(),
            depth: stat.depth(),
            entries: stat.entries(),
            leaf_pages: stat.leaf_pages(),
            overflow_pages: stat.overflow_pages(),
            total_size: stat.total_size(),
            db_portion: stat.total_size() as f64 / self.env.stat()?.total_size() as f64,
        })
    }

    // Returns statistics about the whole database.
    pub(crate) fn get_db_stats(&self) -> DbResult<DbWholeStats> {
        let stat = self.env.stat()?;
        Ok(DbWholeStats {
            entries: stat.entries(),
            branch_pages: stat.branch_pages(),
            leaf_pages: stat.leaf_pages(),
            overflow_pages: stat.overflow_pages(),
            total_size: stat.total_size(),
            page_size: stat.page_size().into(),
            freelist_size: self.env.freelist()?,
        })
    }

    // Returns information about the database.
    pub(crate) fn get_db_info(&self) -> DbResult<Info> {
        Ok(self.env.info()?)
    }

    // Returns the the number of free pages in the database.
    // NOTICE: currently, this function will return a garbage value due to a bug in the binding
    // freelist function.
    // TODO(dvir): bump libmdbx version when the bug is fixed.
    pub(crate) fn get_free_pages(&self) -> DbResult<usize> {
        Ok(self.env.freelist()?)
    }
}

// Serialize bytes as a human readable string.
// For example 1024*1024 bytes will be serialized as "1 MiB".
fn readable_bytes<S>(bytes_num: &u64, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    s.serialize_str(&human_bytes(*bytes_num as f64))
}

// Serialize float with 4 decimal points.
// For example 0.123456 will be serialized to 0.1234.
fn float_precision<S>(float: &f64, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    const PRECISION: u32 = 4;
    let power = u32::pow(10, PRECISION) as f64;
    s.serialize_f64((*float * power).round() / power)
}
