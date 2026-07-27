use std::sync::Arc;

use redbx::{MultimapTableDefinition, ReadableMultimapTable};

use crate::transaction::{RedbxReadTransaction, RedbxWriteTransaction};
use crate::types::{
    RedbxError, RedbxKeyValue, RedbxValue, decode_stored, encode_range, value_to_bytes,
};

// ── Write multimap table ──────────────────────────────────────────────────────

/// A read-write multimap table — each key may have multiple distinct values.
#[derive(uniffi::Object)]
pub struct RedbxMultimapTable {
    name: String,
    txn: Arc<RedbxWriteTransaction>,
}

impl RedbxMultimapTable {
    pub(crate) fn new(name: String, txn: Arc<RedbxWriteTransaction>) -> Self {
        Self { name, txn }
    }

    fn def(&self) -> MultimapTableDefinition<'_, &'static [u8], &'static [u8]> {
        MultimapTableDefinition::new(&self.name)
    }
}

#[uniffi::export]
impl RedbxMultimapTable {
    /// Insert `value` under `key`. No-op if the pair already exists.
    pub fn insert(&self, key: RedbxValue, value: RedbxValue) -> Result<(), RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let val_bytes = value_to_bytes(&value);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let mut table = txn
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        table
            .insert(key_bytes.as_slice(), val_bytes.as_slice())
            .map_err(RedbxError::from)?;
        Ok(())
    }

    /// Return all values stored under `key`.
    pub fn get(&self, key: RedbxValue) -> Result<Vec<RedbxValue>, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let table = txn
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table.get(key_bytes.as_slice()).map_err(RedbxError::from)? {
            let entry = entry.map_err(RedbxError::from)?;
            out.push(decode_stored(entry.value(), "value")?);
        }
        Ok(out)
    }

    /// Remove a specific `key`/`value` pair. Returns `true` if the pair existed.
    pub fn remove(&self, key: RedbxValue, value: RedbxValue) -> Result<bool, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let val_bytes = value_to_bytes(&value);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let mut table = txn
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        table
            .remove(key_bytes.as_slice(), val_bytes.as_slice())
            .map_err(RedbxError::from)
    }

    /// Remove all values under `key`. Returns the number of removed entries.
    pub fn remove_all(&self, key: RedbxValue) -> Result<u64, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let txn = guard.as_ref().ok_or(RedbxError::TransactionConsumed)?;
        let mut table = txn
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        let mut count = 0u64;
        let values: Vec<Vec<u8>> = table
            .get(key_bytes.as_slice())
            .map_err(RedbxError::from)?
            .map(|r| r.map(|e| e.value().to_vec()))
            .collect::<Result<_, _>>()
            .map_err(RedbxError::from)?;
        for val_bytes in values {
            if table
                .remove(key_bytes.as_slice(), val_bytes.as_slice())
                .map_err(RedbxError::from)?
            {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Return all entries whose keys fall in `[start, end]` (inclusive).
    /// Each key appears once per value it holds.
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
        let table = txn
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table
            .range(start_bytes.as_slice()..=end_bytes.as_slice())
            .map_err(RedbxError::from)?
        {
            let (k, values) = entry.map_err(RedbxError::from)?;
            let key = decode_stored(k.value(), "key")?;
            for v in values {
                let v = v.map_err(RedbxError::from)?;
                out.push(RedbxKeyValue {
                    key: key.clone(),
                    value: decode_stored(v.value(), "value")?,
                });
            }
        }
        Ok(out)
    }
}

// ── Read-only multimap table ──────────────────────────────────────────────────

/// A read-only multimap table handle tied to a read transaction.
#[derive(uniffi::Object)]
pub struct RedbxReadOnlyMultimapTable {
    name: String,
    txn: Arc<RedbxReadTransaction>,
}

impl RedbxReadOnlyMultimapTable {
    pub(crate) fn new(name: String, txn: Arc<RedbxReadTransaction>) -> Self {
        Self { name, txn }
    }

    fn def(&self) -> MultimapTableDefinition<'_, &'static [u8], &'static [u8]> {
        MultimapTableDefinition::new(&self.name)
    }
}

#[uniffi::export]
impl RedbxReadOnlyMultimapTable {
    /// Return all values stored under `key`.
    pub fn get(&self, key: RedbxValue) -> Result<Vec<RedbxValue>, RedbxError> {
        let key_bytes = value_to_bytes(&key);
        let guard = self.txn.inner.lock().unwrap();
        let table = guard
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table.get(key_bytes.as_slice()).map_err(RedbxError::from)? {
            let entry = entry.map_err(RedbxError::from)?;
            out.push(decode_stored(entry.value(), "value")?);
        }
        Ok(out)
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
        let table = guard
            .open_multimap_table(self.def())
            .map_err(RedbxError::from)?;
        let mut out = Vec::new();
        for entry in table
            .range(start_bytes.as_slice()..=end_bytes.as_slice())
            .map_err(RedbxError::from)?
        {
            let (k, values) = entry.map_err(RedbxError::from)?;
            let key = decode_stored(k.value(), "key")?;
            for v in values {
                let v = v.map_err(RedbxError::from)?;
                out.push(RedbxKeyValue {
                    key: key.clone(),
                    value: decode_stored(v.value(), "value")?,
                });
            }
        }
        Ok(out)
    }
}
