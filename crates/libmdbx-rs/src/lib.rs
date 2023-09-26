#![allow(clippy::type_complexity)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use crate::{
    codec::*,
    cursor::{Cursor, IntoIter, Iter, IterDup},
    database::{
        Database, DatabaseBuilder, DatabaseKind, Geometry, Info, NoWriteMap, PageSize, Stat,
        WriteMap,
    },
    error::{Error, Result},
    flags::*,
    table::Table,
    transaction::{Transaction, TransactionKind, RO, RW},
};

mod codec;
mod cursor;
mod database;
mod error;
mod flags;
mod table;
mod transaction;

/// Fully typed ORM for use with libmdbx.
#[cfg(feature = "orm")]
#[cfg_attr(docsrs, doc(cfg(feature = "orm")))]
pub mod orm;

#[cfg(feature = "orm")]
mod orm_uses {
    #[doc(hidden)]
    pub use arrayref;

    #[doc(hidden)]
    pub use impls;

    #[cfg(feature = "cbor")]
    #[doc(hidden)]
    pub use ciborium;
}

#[cfg(feature = "orm")]
pub use orm_uses::*;

#[cfg(test)]
mod test_utils {
    use super::*;
    use byteorder::{ByteOrder, LittleEndian};
    use tempfile::tempdir;

    type Database = crate::Database<NoWriteMap>;

    /// Regression test for https://github.com/danburkert/lmdb-rs/issues/21.
    /// This test reliably segfaults when run against lmbdb compiled with opt level -O3 and newer
    /// GCC compilers.
    #[test]
    fn issue_21_regression() {
        const HEIGHT_KEY: [u8; 1] = [0];

        let dir = tempdir().unwrap();

        let db = {
            let mut builder = Database::new();
            builder.set_max_tables(2);
            builder.set_geometry(Geometry {
                size: Some(1_000_000..1_000_000),
                ..Default::default()
            });
            builder.open(dir.path()).unwrap()
        };

        for height in 0..1000 {
            let mut value = [0u8; 8];
            LittleEndian::write_u64(&mut value, height);
            let tx = db.begin_rw_txn().unwrap();
            let index = tx.create_table(None, TableFlags::DUP_SORT).unwrap();
            tx.put(&index, HEIGHT_KEY, value, WriteFlags::empty())
                .unwrap();
            tx.commit().unwrap();
        }
    }
}
