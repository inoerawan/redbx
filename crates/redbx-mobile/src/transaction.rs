use std::sync::{Arc, Mutex};

use redbx::{ReadTransaction, WriteTransaction};

use crate::multimap::{RedbxMultimapTable, RedbxReadOnlyMultimapTable};
use crate::table::{RedbxReadOnlyTable, RedbxTable};
use crate::types::RedbxError;

// ── Write transaction ─────────────────────────────────────────────────────────

/// A write transaction. Must be explicitly committed or aborted.
///
/// Using a table object after calling `commit()` or `abort()` returns
/// [`RedbxError::TransactionConsumed`].
#[derive(uniffi::Object)]
pub struct RedbxWriteTransaction {
    /// `Option` allows the inner `WriteTransaction` to be moved out on commit/abort
    /// while the `Arc` wrapping this struct stays alive (held by table objects).
    pub(crate) inner: Mutex<Option<WriteTransaction>>,
}

impl RedbxWriteTransaction {
    pub(crate) fn new(txn: WriteTransaction) -> Self {
        Self { inner: Mutex::new(Some(txn)) }
    }
}

#[uniffi::export]
impl RedbxWriteTransaction {
    /// Open a typed table by name. Creates it if it does not yet exist.
    pub fn open_table(
        self: Arc<Self>,
        name: String,
    ) -> Result<Arc<RedbxTable>, RedbxError> {
        // Validate that the transaction is still alive before constructing the handle.
        {
            let guard = self.inner.lock().unwrap();
            if guard.is_none() {
                return Err(RedbxError::TransactionConsumed);
            }
        }
        Ok(Arc::new(RedbxTable::new(name, Arc::clone(&self))))
    }

    /// Open a multimap table by name. Creates it if it does not yet exist.
    pub fn open_multimap_table(
        self: Arc<Self>,
        name: String,
    ) -> Result<Arc<RedbxMultimapTable>, RedbxError> {
        {
            let guard = self.inner.lock().unwrap();
            if guard.is_none() {
                return Err(RedbxError::TransactionConsumed);
            }
        }
        Ok(Arc::new(RedbxMultimapTable::new(name, Arc::clone(&self))))
    }

    /// Commit all changes. After this call the transaction is consumed and table handles
    /// will return [`RedbxError::TransactionConsumed`].
    pub fn commit(&self) -> Result<(), RedbxError> {
        let txn = self
            .inner
            .lock()
            .unwrap()
            .take()
            .ok_or(RedbxError::TransactionConsumed)?;
        txn.commit().map_err(|e| RedbxError::UnknownError {
            message: e.to_string(),
        })
    }

    /// Abort all changes. After this call the transaction is consumed.
    pub fn abort(&self) {
        // If already consumed, silently ignore.
        let _ = self.inner.lock().unwrap().take();
    }
}

// ── Read transaction ──────────────────────────────────────────────────────────

/// A read-only snapshot transaction.
#[derive(uniffi::Object)]
pub struct RedbxReadTransaction {
    pub(crate) inner: Mutex<ReadTransaction>,
}

impl RedbxReadTransaction {
    pub(crate) fn new(txn: ReadTransaction) -> Self {
        Self { inner: Mutex::new(txn) }
    }
}

#[uniffi::export]
impl RedbxReadTransaction {
    /// Open a read-only table by name.
    pub fn open_table(
        self: Arc<Self>,
        name: String,
    ) -> Result<Arc<RedbxReadOnlyTable>, RedbxError> {
        Ok(Arc::new(RedbxReadOnlyTable::new(name, Arc::clone(&self))))
    }

    /// Open a read-only multimap table by name.
    pub fn open_multimap_table(
        self: Arc<Self>,
        name: String,
    ) -> Result<Arc<RedbxReadOnlyMultimapTable>, RedbxError> {
        Ok(Arc::new(RedbxReadOnlyMultimapTable::new(
            name,
            Arc::clone(&self),
        )))
    }
}
