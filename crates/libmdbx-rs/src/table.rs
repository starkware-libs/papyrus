use crate::{
    database::DatabaseKind,
    error::{mdbx_result, Result},
    transaction::{txn_execute, TransactionKind},
    Transaction,
};
use libc::c_uint;
use std::{ffi::CString, marker::PhantomData, ptr};

/// A handle to an individual table in a database.
///
/// A table handle denotes the name and parameters of a table in a database.
#[derive(Debug)]
pub struct Table<'txn> {
    dbi: ffi::MDBX_dbi,
    _marker: PhantomData<&'txn ()>,
}

impl<'txn> Table<'txn> {
    pub(crate) fn new<'db, K: TransactionKind, E: DatabaseKind>(
        txn: &'txn Transaction<'db, K, E>,
        name: Option<&str>,
        flags: c_uint,
    ) -> Result<Self> {
        let c_name = name.map(|n| CString::new(n).unwrap());
        let name_ptr = if let Some(c_name) = &c_name {
            c_name.as_ptr()
        } else {
            ptr::null()
        };
        let mut dbi: ffi::MDBX_dbi = 0;
        mdbx_result(txn_execute(&txn.txn_mutex(), |txn| unsafe {
            ffi::mdbx_dbi_open(txn, name_ptr, flags, &mut dbi)
        }))?;
        Ok(Self::new_from_ptr(dbi))
    }

    pub(crate) fn new_from_ptr(dbi: ffi::MDBX_dbi) -> Self {
        Self {
            dbi,
            _marker: PhantomData,
        }
    }

    pub(crate) fn freelist_table() -> Self {
        Table {
            dbi: 0,
            _marker: PhantomData,
        }
    }

    /// Returns the underlying MDBX table handle (dbi).
    ///
    /// The caller **must** ensure that the handle is not used after the lifetime of the
    /// database, or after the table has been closed.
    pub fn dbi(&self) -> ffi::MDBX_dbi {
        self.dbi
    }
}

unsafe impl<'txn> Send for Table<'txn> {}
unsafe impl<'txn> Sync for Table<'txn> {}
