use crate::{
    database::{Database, DatabaseKind, NoWriteMap, TxnManagerMessage, TxnPtr},
    error::{mdbx_result, Result},
    flags::{TableFlags, WriteFlags},
    table::Table,
    Cursor, Decodable, Error, Stat,
};
use ffi::{MDBX_txn_flags_t, MDBX_TXN_RDONLY, MDBX_TXN_READWRITE};
use indexmap::IndexSet;
use libc::{c_uint, c_void};
use parking_lot::Mutex;
use std::{
    fmt,
    fmt::Debug,
    marker::PhantomData,
    mem::{self, size_of},
    ptr, result, slice,
    sync::{mpsc::sync_channel, Arc},
};

mod private {
    use super::*;

    pub trait Sealed {}

    impl Sealed for RO {}
    impl Sealed for RW {}
}

pub trait TransactionKind: private::Sealed + Debug + 'static {
    #[doc(hidden)]
    const ONLY_CLEAN: bool;

    #[doc(hidden)]
    const OPEN_FLAGS: MDBX_txn_flags_t;
}

#[derive(Debug)]
pub struct RO;
#[derive(Debug)]
pub struct RW;

impl TransactionKind for RO {
    const ONLY_CLEAN: bool = true;
    const OPEN_FLAGS: MDBX_txn_flags_t = MDBX_TXN_RDONLY;
}
impl TransactionKind for RW {
    const ONLY_CLEAN: bool = false;
    const OPEN_FLAGS: MDBX_txn_flags_t = MDBX_TXN_READWRITE;
}

#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct CommitLatencyPointer(pub *mut ffi::MDBX_commit_latency);

unsafe impl Send for CommitLatencyPointer {}
unsafe impl Sync for CommitLatencyPointer {}
#[repr(transparent)]
#[derive(Debug, Clone)]
pub struct CommitLatency(ffi::MDBX_commit_latency);

unsafe impl Send for CommitLatency {}
unsafe impl Sync for CommitLatency {}

//unsafe impl Send for ffi::MDBX_commit_latency {}

impl CommitLatency {
    /// Create a new CommitLatency with zero'd inner struct `ffi::MDB_stat`.
    pub(crate) fn new() -> CommitLatency {
        unsafe { CommitLatency(mem::zeroed()) }
    }

    /// Returns a mut pointer to `ffi::MDBX_commit_latency`.
    pub(crate) fn mdbx_commit_latency(&mut self) -> *mut ffi::MDBX_commit_latency {
        &mut self.0
    }

    pub fn print_latency(&self) {
        println!("{:#?}", self.0);
        // println!("whole: {}", self.0.whole as u32);
        // println!("audit: {}", self.0.audit as u32);
        // println!("ending: {}", self.0.ending as u32);
        // println!("gc_wallclock: {}", self.0.gc_wallclock as u32);
        // println!("gc_cputime: {}", self.0.gc_cputime as u32);
        // println!("preparation: {}", self.0.preparation as u32);
        // println!("sync: {}", self.0.sync as u32);
        // println!("write: {}", self.0.write as u32);
        // let x=self.0.gc_prof;
        // println!("===========");
        // println!("xxx: {}", x.flushes as u32);
        // println!("gc_prof: {:?}", x);
    }

    pub fn get_whole(&self) -> u32 {
        self.0.whole as u32
    }

    pub fn get_audit(&self) -> u32 {
        self.0.audit as u32
    }

    pub fn get_ending(&self) -> u32 {
        self.0.ending as u32
    }

    pub fn get_gc_wallclock(&self) -> u32 {
        self.0.gc_wallclock as u32
    }

    pub fn get_gc_cputime(&self) -> u32 {
        self.0.gc_cputime as u32
    }

    pub fn get_preparation(&self) -> u32 {
        self.0.preparation as u32
    }

    pub fn get_sync(&self) -> u32 {
        self.0.sync as u32
    }

    pub fn get_write(&self) -> u32 {
        self.0.write as u32
    }
}
/// An MDBX transaction.
///
/// All table operations require a transaction.
pub struct Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
    txn: Arc<Mutex<*mut ffi::MDBX_txn>>,
    primed_dbis: Mutex<IndexSet<ffi::MDBX_dbi>>,
    committed: bool,
    db: &'db Database<E>,
    _marker: PhantomData<fn(K)>,
}

impl<'db, K, E> Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
    pub(crate) fn new(db: &'db Database<E>) -> Result<Self> {
        let mut txn: *mut ffi::MDBX_txn = ptr::null_mut();
        unsafe {
            mdbx_result(ffi::mdbx_txn_begin_ex(
                db.ptr(),
                ptr::null_mut(),
                K::OPEN_FLAGS,
                &mut txn,
                ptr::null_mut(),
            ))?;
            Ok(Self::new_from_ptr(db, txn))
        }
    }

    pub(crate) fn new_from_ptr(db: &'db Database<E>, txn: *mut ffi::MDBX_txn) -> Self {
        Self {
            txn: Arc::new(Mutex::new(txn)),
            primed_dbis: Mutex::new(IndexSet::new()),
            committed: false,
            db,
            _marker: PhantomData,
        }
    }

    /// Returns a raw pointer to the underlying MDBX transaction.
    ///
    /// The caller **must** ensure that the pointer is not used after the
    /// lifetime of the transaction.
    pub(crate) fn txn_mutex(&self) -> Arc<Mutex<*mut ffi::MDBX_txn>> {
        self.txn.clone()
    }

    pub fn txn(&self) -> *mut ffi::MDBX_txn {
        *self.txn.lock()
    }

    /// Returns a raw pointer to the MDBX database.
    pub fn db(&self) -> &Database<E> {
        self.db
    }

    /// Returns the transaction id.
    pub fn id(&self) -> u64 {
        txn_execute(&self.txn, |txn| unsafe { ffi::mdbx_txn_id(txn) })
    }

    /// Gets an item from a table.
    ///
    /// This function retrieves the data associated with the given key in the
    /// table. If the table supports duplicate keys
    /// ([TableFlags::DUP_SORT]) then the first data item for the key will be
    /// returned. Retrieval of other items requires the use of
    /// [Cursor]. If the item is not in the table, then
    /// [None] will be returned.
    pub fn get<'txn, Key>(&'txn self, table: &Table<'txn>, key: &[u8]) -> Result<Option<Key>>
    where
        Key: Decodable<'txn>,
    {
        let key_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: key.len(),
            iov_base: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: 0,
            iov_base: ptr::null_mut(),
        };

        txn_execute(&self.txn, |txn| unsafe {
            match ffi::mdbx_get(txn, table.dbi(), &key_val, &mut data_val) {
                ffi::MDBX_SUCCESS => Key::decode_val::<K>(txn, &data_val).map(Some),
                ffi::MDBX_NOTFOUND => Ok(None),
                err_code => Err(Error::from_err_code(err_code)),
            }
        })
    }

    /// Commits the transaction.
    ///
    /// Any pending operations will be saved.
    pub fn commit(self) -> Result<(bool, CommitLatency)> {
        self.commit_and_rebind_open_dbs().map(|v| (v.0, v.1))
    }

    pub fn prime_for_permaopen(&self, table: Table<'_>) {
        self.primed_dbis.lock().insert(table.dbi());
    }

    /// Commits the transaction and returns table handles permanently open for the lifetime of `Database`.
    pub fn commit_and_rebind_open_dbs(mut self) -> Result<(bool, CommitLatency, Vec<Table<'db>>)> {
        let txnlck = self.txn.lock();
        let txn = *txnlck;
        let mut txn_latency = CommitLatency::new();
        let latency_pointer = CommitLatencyPointer(txn_latency.mdbx_commit_latency());
        let result = if K::ONLY_CLEAN {
            mdbx_result(unsafe { ffi::mdbx_txn_commit_ex(txn, latency_pointer.0) })
        } else {
            let (sender, rx) = sync_channel(0);
            self.db
                .txn_manager
                .as_ref()
                .unwrap()
                .send(TxnManagerMessage::Commit {
                    tx: TxnPtr(txn),
                    sender,
                    commit_latency_pointer: latency_pointer,
                })
                .unwrap();
            rx.recv().unwrap()
        };
        self.committed = true;
        result.map(|v| {
            (
                v,
                txn_latency,
                self.primed_dbis
                    .lock()
                    .iter()
                    .map(|&dbi| Table::new_from_ptr(dbi))
                    .collect(),
            )
        })
    }

    /// Opens a handle to an MDBX table.
    ///
    /// If `name` is [None], then the returned handle will be for the default table.
    ///
    /// If `name` is not [None], then the returned handle will be for a named table. In this
    /// case the database must be configured to allow named tables through
    /// [DatabaseBuilder::set_max_tables()](crate::DatabaseBuilder::set_max_tables).
    ///
    /// The returned table handle may be shared among any transaction in the database.
    ///
    /// The table name may not contain the null character.
    pub fn open_table<'txn>(&'txn self, name: Option<&str>) -> Result<Table<'txn>> {
        Table::new(self, name, 0)
    }

    /// Gets the option flags for the given table in the transaction.
    pub fn table_flags<'txn>(&'txn self, table: &Table<'txn>) -> Result<TableFlags> {
        let mut flags: c_uint = 0;
        unsafe {
            mdbx_result(txn_execute(&self.txn, |txn| {
                ffi::mdbx_dbi_flags_ex(txn, table.dbi(), &mut flags, ptr::null_mut())
            }))?;
        }
        Ok(TableFlags::from_bits_truncate(flags))
    }

    /// Retrieves table statistics.
    pub fn table_stat<'txn>(&'txn self, table: &Table<'txn>) -> Result<Stat> {
        unsafe {
            let mut stat = Stat::new();
            mdbx_result(txn_execute(&self.txn, |txn| {
                ffi::mdbx_dbi_stat(txn, table.dbi(), stat.mdb_stat(), size_of::<Stat>())
            }))?;
            Ok(stat)
        }
    }

    /// Open a new cursor on the given table.
    pub fn cursor<'txn>(&'txn self, table: &Table<'txn>) -> Result<Cursor<'txn, K>> {
        Cursor::new(self, table)
    }
}

pub(crate) fn txn_execute<F: FnOnce(*mut ffi::MDBX_txn) -> T, T>(
    txn: &Mutex<*mut ffi::MDBX_txn>,
    f: F,
) -> T {
    let lck = txn.lock();
    (f)(*lck)
}

impl<'db, E> Transaction<'db, RW, E>
where
    E: DatabaseKind,
{
    fn open_table_with_flags<'txn>(
        &'txn self,
        name: Option<&str>,
        flags: TableFlags,
    ) -> Result<Table<'txn>> {
        Table::new(self, name, flags.bits())
    }

    /// Opens a handle to an MDBX table, creating the table if necessary.
    ///
    /// If the table is already created, the given option flags will be added to it.
    ///
    /// If `name` is [None], then the returned handle will be for the default table.
    ///
    /// If `name` is not [None], then the returned handle will be for a named table. In this
    /// case the database must be configured to allow named tables through
    /// [DatabaseBuilder::set_max_tables()](crate::DatabaseBuilder::set_max_tables).
    ///
    /// This function will fail with [Error::BadRslot](crate::error::Error::BadRslot) if called by a thread with an open
    /// transaction.
    pub fn create_table<'txn>(
        &'txn self,
        name: Option<&str>,
        flags: TableFlags,
    ) -> Result<Table<'txn>> {
        self.open_table_with_flags(name, flags | TableFlags::CREATE)
    }

    /// Stores an item into a table.
    ///
    /// This function stores key/data pairs in the table. The default
    /// behavior is to enter the new key/data pair, replacing any previously
    /// existing key if duplicates are disallowed, or adding a duplicate data
    /// item if duplicates are allowed ([TableFlags::DUP_SORT]).
    pub fn put<'txn>(
        &'txn self,
        table: &Table<'txn>,
        key: impl AsRef<[u8]>,
        data: impl AsRef<[u8]>,
        flags: WriteFlags,
    ) -> Result<()> {
        let key = key.as_ref();
        let data = data.as_ref();
        let key_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: key.len(),
            iov_base: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: data.len(),
            iov_base: data.as_ptr() as *mut c_void,
        };
        mdbx_result(txn_execute(&self.txn, |txn| unsafe {
            ffi::mdbx_put(txn, table.dbi(), &key_val, &mut data_val, flags.bits())
        }))?;

        Ok(())
    }

    /// Returns a buffer which can be used to write a value into the item at the
    /// given key and with the given length. The buffer must be completely
    /// filled by the caller.
    pub fn reserve<'txn>(
        &'txn self,
        table: &Table<'txn>,
        key: impl AsRef<[u8]>,
        len: usize,
        flags: WriteFlags,
    ) -> Result<&'txn mut [u8]> {
        let key = key.as_ref();
        let key_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: key.len(),
            iov_base: key.as_ptr() as *mut c_void,
        };
        let mut data_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: len,
            iov_base: ptr::null_mut::<c_void>(),
        };
        unsafe {
            mdbx_result(txn_execute(&self.txn, |txn| {
                ffi::mdbx_put(
                    txn,
                    table.dbi(),
                    &key_val,
                    &mut data_val,
                    flags.bits() | ffi::MDBX_RESERVE,
                )
            }))?;
            Ok(slice::from_raw_parts_mut(
                data_val.iov_base as *mut u8,
                data_val.iov_len,
            ))
        }
    }

    /// Delete items from a table.
    /// This function removes key/data pairs from the table.
    ///
    /// The data parameter is NOT ignored regardless the table does support sorted duplicate data items or not.
    /// If the data parameter is [Some] only the matching data item will be deleted.
    /// Otherwise, if data parameter is [None], any/all value(s) for specified key will be deleted.
    ///
    /// Returns `true` if the key/value pair was present.
    pub fn del<'txn>(
        &'txn self,
        table: &Table<'txn>,
        key: impl AsRef<[u8]>,
        data: Option<&[u8]>,
    ) -> Result<bool> {
        let key = key.as_ref();
        let key_val: ffi::MDBX_val = ffi::MDBX_val {
            iov_len: key.len(),
            iov_base: key.as_ptr() as *mut c_void,
        };
        let data_val: Option<ffi::MDBX_val> = data.map(|data| ffi::MDBX_val {
            iov_len: data.len(),
            iov_base: data.as_ptr() as *mut c_void,
        });

        mdbx_result({
            txn_execute(&self.txn, |txn| {
                if let Some(d) = data_val {
                    unsafe { ffi::mdbx_del(txn, table.dbi(), &key_val, &d) }
                } else {
                    unsafe { ffi::mdbx_del(txn, table.dbi(), &key_val, ptr::null()) }
                }
            })
        })
        .map(|_| true)
        .or_else(|e| match e {
            Error::NotFound => Ok(false),
            other => Err(other),
        })
    }

    /// Empties the given table. All items will be removed.
    pub fn clear_table<'txn>(&'txn self, table: &Table<'txn>) -> Result<()> {
        mdbx_result(txn_execute(&self.txn, |txn| unsafe {
            ffi::mdbx_drop(txn, table.dbi(), false)
        }))?;

        Ok(())
    }

    /// Drops the table from the database.
    ///
    /// # Safety
    /// Caller must close ALL other [Table] and [Cursor] instances pointing to the same dbi BEFORE calling this function.
    pub unsafe fn drop_table<'txn>(&'txn self, table: Table<'txn>) -> Result<()> {
        mdbx_result(txn_execute(&self.txn, |txn| {
            ffi::mdbx_drop(txn, table.dbi(), true)
        }))?;

        Ok(())
    }
}

impl<'db, E> Transaction<'db, RO, E>
where
    E: DatabaseKind,
{
    /// Closes the table handle.
    ///
    /// # Safety
    /// Caller must close ALL other [Table] and [Cursor] instances pointing to the same dbi BEFORE calling this function.
    pub unsafe fn close_table(&self, table: Table<'_>) -> Result<()> {
        mdbx_result(ffi::mdbx_dbi_close(self.db.ptr(), table.dbi()))?;

        Ok(())
    }
}

impl<'db> Transaction<'db, RW, NoWriteMap> {
    /// Begins a new nested transaction inside of this transaction.
    pub fn begin_nested_txn(&mut self) -> Result<Transaction<'_, RW, NoWriteMap>> {
        txn_execute(&self.txn, |txn| {
            let (tx, rx) = sync_channel(0);
            self.db
                .txn_manager
                .as_ref()
                .unwrap()
                .send(TxnManagerMessage::Begin {
                    parent: TxnPtr(txn),
                    flags: RW::OPEN_FLAGS,
                    sender: tx,
                })
                .unwrap();

            rx.recv()
                .unwrap()
                .map(|ptr| Transaction::new_from_ptr(self.db, ptr.0))
        })
    }
}

impl<'db, K, E> fmt::Debug for Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> result::Result<(), fmt::Error> {
        f.debug_struct("RoTransaction").finish()
    }
}

impl<'db, K, E> Drop for Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
    fn drop(&mut self) {
        txn_execute(&self.txn, |txn| {
            if !self.committed {
                if K::ONLY_CLEAN {
                    unsafe {
                        ffi::mdbx_txn_abort(txn);
                    }
                } else {
                    let (sender, rx) = sync_channel(0);
                    self.db
                        .txn_manager
                        .as_ref()
                        .unwrap()
                        .send(TxnManagerMessage::Abort {
                            tx: TxnPtr(txn),
                            sender,
                        })
                        .unwrap();
                    rx.recv().unwrap().unwrap();
                }
            }
        })
    }
}

unsafe impl<'db, K, E> Send for Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
}

unsafe impl<'db, K, E> Sync for Transaction<'db, K, E>
where
    K: TransactionKind,
    E: DatabaseKind,
{
}
