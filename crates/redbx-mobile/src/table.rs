use std::sync::Arc;

use redbx::{ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::transaction::{RedbxReadTransaction, RedbxWriteTransaction};
use crate::types::{
    RedbxError, RedbxKeyValue, RedbxValue, decode_stored, encode_range, value_to_bytes,
};

// ── Write table ───────────────────────────────────────────────────────────────

/// A read-write table handle tied to a write transaction.
///
/// Re-opens the underlying redbx table on each operation using the cached name.
/// Operations fail with [`RedbxError::TransactionConsumed`] after the parent
/// transaction is committed or aborted.
#[derive(uniffi::Object)]
pub struct RedbxTable {
    name: String,
    txn: Arc<RedbxWriteTransaction>,
}

impl RedbxTable {
    pub(crate) fn new(name: String, txn: Arc<RedbxWriteTransaction>) -> Self {
        Self { name, txn }
    }

    fn def(&self) -> TableDefinition<'_, &'static [u8], &'static [u8]> {
        TableDefinition::new(&self.name)
    }
}

#[uniffi::export]
impl RedbxTable {
    /// Insert a key-value pair, overwriting any existing value for that key.
    pub fn insert(&self, key: RedbxValue, value: RedbxValue) -> Result<(), RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let val_bytes = value_to_bytes(&value);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let mut table = txn.open_table(self.def()).map_err(RedbxError::from)?;
        table
            .insert(key_bytes.as_slice(), val_bytes.as_slice())
            .map_err(RedbxError::from)?;
        Ok(())
    }

    /// Retrieve the value for `key`, or `None` if not present.
    pub fn get(&self, key: RedbxValue) -> Result<Option<RedbxValue>, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let table = txn.open_table(self.def()).map_err(RedbxError::from)?;
        match table.get(key_bytes.as_slice()).map_err(RedbxError::from)? {
            Some(v) => Ok(Some(decode_stored(v.value(), "value")?)),
            None => Ok(None),
        }
    }

    /// Remove the entry for `key`, returning the previous value if present.
    pub fn remove(&self, key: RedbxValue) -> Result<Option<RedbxValue>, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let mut table = txn.open_table(self.def()).map_err(RedbxError::from)?;
        match table
            .remove(key_bytes.as_slice())
            .map_err(RedbxError::from)?
        {
            Some(v) => Ok(Some(decode_stored(v.value(), "value")?)),
            None => Ok(None),
        }
    }

    /// Return all entries whose keys fall in `[start, end]` (inclusive).
    ///
    /// Both endpoints must be the same [`RedbxValue`] variant; otherwise
    /// [`RedbxError::InvalidRange`] is returned.
    ///
    /// The whole result set is materialised in memory — bound the range on
    /// large tables.
    pub fn range(
        &self,
        start: RedbxValue,
        end: RedbxValue,
    ) -> Result<Vec<RedbxKeyValue>, RedbxError> {
        let (start_bytes, end_bytes) = encode_range(&start, &end)?;
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let table = txn.open_table(self.def()).map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table
            .range(start_bytes.as_slice()..=end_bytes.as_slice())
            .map_err(RedbxError::from)?
        {
            let (k, v) = entry.map_err(RedbxError::from)?;
            out.push(RedbxKeyValue {
                key: decode_stored(k.value(), "key")?,
                value: decode_stored(v.value(), "value")?,
            });
        }
        Ok(out)
    }

    /// Return the number of entries in the table.
    pub fn len(&self) -> Result<u64, RedbxError> {
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let table = txn.open_table(self.def()).map_err(RedbxError::from)?;
        table.len().map_err(RedbxError::from)
    }

    /// Returns `true` if the table has no entries.
    pub fn is_empty(&self) -> Result<bool, RedbxError> {
        self.len().map(|n| n == 0)
    }
}

// ── Read-only table ───────────────────────────────────────────────────────────

/// A read-only table handle tied to a read transaction.
#[derive(uniffi::Object)]
pub struct RedbxReadOnlyTable {
    name: String,
    txn: Arc<RedbxReadTransaction>,
}

impl RedbxReadOnlyTable {
    pub(crate) fn new(name: String, txn: Arc<RedbxReadTransaction>) -> Self {
        Self { name, txn }
    }

    fn def(&self) -> TableDefinition<'_, &'static [u8], &'static [u8]> {
        TableDefinition::new(&self.name)
    }
}

#[uniffi::export]
impl RedbxReadOnlyTable {
    /// Retrieve the value for `key`, or `None` if not present.
    pub fn get(&self, key: RedbxValue) -> Result<Option<RedbxValue>, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let table = guard.open_table(self.def()).map_err(RedbxError::from)?;
        match table.get(key_bytes.as_slice()).map_err(RedbxError::from)? {
            Some(v) => Ok(Some(decode_stored(v.value(), "value")?)),
            None => Ok(None),
        }
    }

    /// Return all entries whose keys fall in `[start, end]` (inclusive).
    ///
    /// Both endpoints must be the same [`RedbxValue`] variant; otherwise
    /// [`RedbxError::InvalidRange`] is returned.
    ///
    /// The whole result set is materialised in memory — bound the range on
    /// large tables.
    pub fn range(
        &self,
        start: RedbxValue,
        end: RedbxValue,
    ) -> Result<Vec<RedbxKeyValue>, RedbxError> {
        let (start_bytes, end_bytes) = encode_range(&start, &end)?;
        let guard = self.txn.inner.lock().unwrap();
        let table = guard.open_table(self.def()).map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table
            .range(start_bytes.as_slice()..=end_bytes.as_slice())
            .map_err(RedbxError::from)?
        {
            let (k, v) = entry.map_err(RedbxError::from)?;
            out.push(RedbxKeyValue {
                key: decode_stored(k.value(), "key")?,
                value: decode_stored(v.value(), "value")?,
            });
        }
        Ok(out)
    }

    /// Return the number of entries in the table.
    pub fn len(&self) -> Result<u64, RedbxError> {
        let guard = self.txn.inner.lock().unwrap();
        let table = guard.open_table(self.def()).map_err(RedbxError::from)?;
        table.len().map_err(RedbxError::from)
    }

    /// Returns `true` if the table has no entries.
    pub fn is_empty(&self) -> Result<bool, RedbxError> {
        self.len().map(|n| n == 0)
    }
}
