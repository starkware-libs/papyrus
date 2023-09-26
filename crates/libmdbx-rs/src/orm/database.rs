use super::{traits::*, transaction::Transaction};
use crate::{Geometry, TableFlags, WriteMap, RO, RW};
use anyhow::Context;
use std::{
    collections::BTreeMap,
    fs::DirBuilder,
    ops::Deref,
    path::{Path, PathBuf},
};
use tempfile::tempdir;

#[derive(Debug)]
enum DbFolder {
    Persisted(std::path::PathBuf),
    Temporary(tempfile::TempDir),
}

impl DbFolder {
    fn path(&self) -> &Path {
        match self {
            Self::Persisted(p) => p.as_path(),
            Self::Temporary(temp_dir) => temp_dir.path(),
        }
    }
}

#[derive(Debug)]
pub struct Database {
    inner: crate::Database<WriteMap>,
    folder: DbFolder,
}

impl Database {
    pub fn path(&self) -> &Path {
        self.folder.path()
    }

    fn open_db(
        mut builder: crate::DatabaseBuilder<WriteMap>,
        folder: DbFolder,
        chart: &DatabaseChart,
        read_only: bool,
    ) -> anyhow::Result<Self> {
        builder.set_max_tables(std::cmp::max(chart.len(), 1));

        builder.set_flags(crate::DatabaseFlags {
            mode: if read_only {
                crate::Mode::ReadOnly
            } else {
                crate::Mode::ReadWrite {
                    sync_mode: crate::SyncMode::Durable,
                }
            },
            no_rdahead: true,
            coalesce: true,
            ..Default::default()
        });

        Ok(Self {
            inner: builder.open(folder.path()).with_context(|| {
                format!("failed to open database at {}", folder.path().display())
            })?,
            folder,
        })
    }

    fn new(chart: &DatabaseChart, folder: DbFolder, read_only: bool) -> anyhow::Result<Self> {
        let mut builder = crate::Database::<WriteMap>::new();
        builder.set_max_tables(chart.len());
        builder.set_geometry(Geometry {
            size: Some(..isize::MAX as usize),
            growth_step: None,
            shrink_threshold: None,
            page_size: None,
        });
        builder.set_rp_augment_limit(16 * 256 * 1024);
        if read_only {
            Self::open_db(builder, folder, chart, true)
        } else {
            let _ = DirBuilder::new().recursive(true).create(folder.path());

            let this = Self::open_db(builder, folder, chart, false)?;

            let tx = this.inner.begin_rw_txn()?;
            for (table, settings) in chart {
                tx.create_table(
                    Some(table),
                    if settings.dup_sort {
                        TableFlags::DUP_SORT
                    } else {
                        TableFlags::default()
                    },
                )?;
            }
            tx.commit()?;

            Ok(this)
        }
    }

    pub fn create(chart: &DatabaseChart, path: Option<PathBuf>) -> anyhow::Result<Database> {
        let folder = if let Some(path) = path {
            DbFolder::Persisted(path)
        } else {
            let path = tempdir()?;
            DbFolder::Temporary(path)
        };

        Self::new(chart, folder, false)
    }

    pub fn open(chart: &DatabaseChart, path: &Path) -> anyhow::Result<Database> {
        Self::new(chart, DbFolder::Persisted(path.to_path_buf()), true)
    }
}

impl Deref for Database {
    type Target = crate::Database<WriteMap>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Database {
    pub fn begin_read(&self) -> anyhow::Result<Transaction<'_, RO>> {
        Ok(Transaction {
            inner: self.inner.begin_ro_txn()?,
        })
    }

    pub fn begin_readwrite(&self) -> anyhow::Result<Transaction<'_, RW>> {
        Ok(Transaction {
            inner: self.inner.begin_rw_txn()?,
        })
    }
}

#[derive(Debug)]
pub struct UntypedTable<T>(pub T)
where
    T: Table;

impl<T> Table for UntypedTable<T>
where
    T: Table,
{
    const NAME: &'static str = T::NAME;

    type Key = Vec<u8>;
    type Value = Vec<u8>;
    type SeekKey = Vec<u8>;
}

impl<T> UntypedTable<T>
where
    T: Table,
{
    pub fn encode_key(key: T::Key) -> <<T as Table>::Key as Encodable>::Encoded {
        key.encode()
    }

    pub fn decode_key(encoded: &[u8]) -> anyhow::Result<T::Key>
    where
        <T as Table>::Key: Decodable,
    {
        <T::Key as Decodable>::decode(encoded)
    }

    pub fn encode_value(value: T::Value) -> <<T as Table>::Value as Encodable>::Encoded {
        value.encode()
    }

    pub fn decode_value(encoded: &[u8]) -> anyhow::Result<T::Value> {
        <T::Value as Decodable>::decode(encoded)
    }

    pub fn encode_seek_key(value: T::SeekKey) -> <<T as Table>::SeekKey as Encodable>::Encoded {
        value.encode()
    }
}

#[macro_export]
macro_rules! table {
    ($(#[$docs:meta])+ ( $name:ident ) $key:ty [ $seek_key:ty ] => $value:ty) => {
        $(#[$docs])+
        ///
        #[doc = concat!("Takes [`", stringify!($key), "`] as a key and returns [`", stringify!($value), "`]")]
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name;

        impl $crate::orm::Table for $name {
            const NAME: &'static str = stringify!($name);

            type Key = $key;
            type SeekKey = $seek_key;
            type Value = $value;
        }

        impl $name {
            pub const fn untyped(self) -> $crate::orm::UntypedTable<Self> {
                $crate::orm::UntypedTable(self)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", <Self as $crate::orm::Table>::NAME)
            }
        }
    };
    ($(#[$docs:meta])+ ( $name:ident ) $key:ty => $value:ty) => {
        table!(
            $(#[$docs])+
            ( $name ) $key [ $key ] => $value
        );
    };
}

#[macro_export]
macro_rules! dupsort {
    ($(#[$docs:meta])+ ( $table_name:ident ) $key:ty [$seek_key:ty] => $value:ty [$seek_value:ty] ) => {
        table!(
            $(#[$docs])+
            ///
            #[doc = concat!("`DUPSORT` table with seek value type being: [`", stringify!($seek_value), "`].")]
            ( $table_name ) $key [$seek_key] => $value
        );
        impl $crate::orm::DupSort for $table_name {
            type SeekValue = $seek_value;
        }
    };

    ($(#[$docs:meta])+ ( $table_name:ident ) $key:ty [$seek_key:ty] => $value:ty ) => {
        dupsort!(
            $(#[$docs])+
            ( $table_name ) $key [$seek_key] => $value [$value]
        );
    };

    ($(#[$docs:meta])+ ( $table_name:ident ) $key:ty => $value:ty [$seek_value:ty] ) => {
        dupsort!(
            $(#[$docs])+
            ( $table_name ) $key [$key] => $value [$seek_value]
        );
    };

    ($(#[$docs:meta])+ ( $table_name:ident ) $key:ty => $value:ty ) => {
        dupsort!(
            $(#[$docs])+
            ( $table_name ) $key [$key] => $value [$value]
        );
    };
}

#[derive(Clone, Debug, Default)]
pub struct TableSettings {
    pub dup_sort: bool,
}

/// Contains settings for each table in the database to be created or opened.
pub type DatabaseChart = BTreeMap<&'static str, TableSettings>;

#[macro_export]
macro_rules! table_info {
    ($t:ty) => {
        (
            <$t as $crate::orm::Table>::NAME,
            $crate::orm::TableSettings {
                dup_sort: $crate::impls::impls!($t: $crate::orm::DupSort),
            },
        )
    };
}
