use std::sync::{Arc, RwLock};

use redbx::{Database, ReadableDatabase};

use crate::transaction::{RedbxReadTransaction, RedbxWriteTransaction};
use crate::types::RedbxError;

/// An encrypted redbx database handle.
///
/// Thread-safe — safe to share across threads. Create transactions per operation.
///
/// The lock is an [`RwLock`] rather than a `Mutex` because only [`Self::compact`]
/// needs exclusive access to the `Database`. Beginning a transaction takes a
/// shared guard, so a reader never has to queue behind a writer that is still
/// waiting for redbx's single write slot.
#[derive(uniffi::Object)]
pub struct RedbxDatabase {
    inner: RwLock<Database>,
}

impl RedbxDatabase {
    fn wrap(db: Database) -> Arc<Self> {
        Arc::new(Self {
            inner: RwLock::new(db),
        })
    }
}

#[uniffi::export]
impl RedbxDatabase {
    /// Create a new encrypted database at `path` protected by `password`.
    ///
    /// Fails if the file already exists and is not a valid redbx database.
    #[uniffi::constructor]
    pub fn create(path: String, password: String) -> Result<Arc<Self>, RedbxError> {
        let db = Database::create(path, &password).map_err(RedbxError::from)?;
        Ok(Self::wrap(db))
    }

    /// Open an existing encrypted database at `path` with `password`.
    ///
    /// Returns [`RedbxError::IncorrectPassword`] if the password is wrong.
    #[uniffi::constructor]
    pub fn open(path: String, password: String) -> Result<Arc<Self>, RedbxError> {
        let db = Database::open(path, &password).map_err(RedbxError::from)?;
        Ok(Self::wrap(db))
    }

    /// Begin a write transaction. Only one write transaction may be active at a time,
    /// so this blocks until any in-flight write transaction is committed or aborted.
    pub fn begin_write(&self) -> Result<Arc<RedbxWriteTransaction>, RedbxError> {
        let db = self.inner.read().unwrap();
        let txn = db.begin_write().map_err(|e| RedbxError::UnknownError {
            detail: e.to_string(),
        })?;
        Ok(Arc::new(RedbxWriteTransaction::new(txn)))
    }

    /// Begin a read-only transaction. Multiple read transactions may be active simultaneously.
    pub fn begin_read(&self) -> Result<Arc<RedbxReadTransaction>, RedbxError> {
        let db = self.inner.read().unwrap();
        let txn = db.begin_read().map_err(|e| RedbxError::UnknownError {
            detail: e.to_string(),
        })?;
        Ok(Arc::new(RedbxReadTransaction::new(txn)))
    }

    /// Compact the database, reclaiming freed pages. Returns `true` if compaction occurred.
    ///
    /// Takes exclusive access: no transaction can be started while this runs, and any
    /// live transaction must be finished first.
    pub fn compact(&self) -> Result<bool, RedbxError> {
        let mut db = self.inner.write().unwrap();
        db.compact().map_err(|e| RedbxError::UnknownError {
            detail: e.to_string(),
        })
    }
}
