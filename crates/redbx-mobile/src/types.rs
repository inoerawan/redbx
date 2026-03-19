use std::fmt;

// ── Value type ────────────────────────────────────────────────────────────────

/// A typed value that can be stored as a key or value in redbx tables.
///
/// Serialized on the wire as `[1-byte tag][little-endian payload]`.
/// The tag is stored alongside the data so reads reconstruct the correct variant.
#[derive(uniffi::Enum, Debug, Clone, PartialEq)]
pub enum RedbxValue {
    Bytes(Vec<u8>),
    Str(String),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Bool(bool),
}

// Tag constants
const TAG_BYTES: u8 = 0x00;
const TAG_STR: u8 = 0x01;
const TAG_U8: u8 = 0x02;
const TAG_U16: u8 = 0x03;
const TAG_U32: u8 = 0x04;
const TAG_U64: u8 = 0x05;
const TAG_I8: u8 = 0x06;
const TAG_I16: u8 = 0x07;
const TAG_I32: u8 = 0x08;
const TAG_I64: u8 = 0x09;
const TAG_F32: u8 = 0x0A;
const TAG_F64: u8 = 0x0B;
const TAG_BOOL: u8 = 0x0C;

/// Serialize a [`RedbxValue`] to tagged bytes for storage.
pub fn value_to_bytes(v: &RedbxValue) -> Vec<u8> {
    match v {
        RedbxValue::Bytes(b) => {
            let mut out = Vec::with_capacity(1 + b.len());
            out.push(TAG_BYTES);
            out.extend_from_slice(b);
            out
        }
        RedbxValue::Str(s) => {
            let b = s.as_bytes();
            let mut out = Vec::with_capacity(1 + b.len());
            out.push(TAG_STR);
            out.extend_from_slice(b);
            out
        }
        RedbxValue::U8(n) => vec![TAG_U8, *n],
        RedbxValue::U16(n) => {
            let mut out = vec![TAG_U16];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::U32(n) => {
            let mut out = vec![TAG_U32];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::U64(n) => {
            let mut out = vec![TAG_U64];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::I8(n) => vec![TAG_I8, *n as u8],
        RedbxValue::I16(n) => {
            let mut out = vec![TAG_I16];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::I32(n) => {
            let mut out = vec![TAG_I32];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::I64(n) => {
            let mut out = vec![TAG_I64];
            out.extend_from_slice(&n.to_le_bytes());
            out
        }
        RedbxValue::F32(f) => {
            let mut out = vec![TAG_F32];
            out.extend_from_slice(&f.to_le_bytes());
            out
        }
        RedbxValue::F64(f) => {
            let mut out = vec![TAG_F64];
            out.extend_from_slice(&f.to_le_bytes());
            out
        }
        RedbxValue::Bool(b) => vec![TAG_BOOL, u8::from(*b)],
    }
}

/// Deserialize a [`RedbxValue`] from tagged bytes. Returns `None` on corrupt data.
pub fn bytes_to_value(data: &[u8]) -> Option<RedbxValue> {
    let (&tag, payload) = data.split_first()?;
    match tag {
        TAG_BYTES => Some(RedbxValue::Bytes(payload.to_vec())),
        TAG_STR => Some(RedbxValue::Str(
            String::from_utf8(payload.to_vec()).ok()?,
        )),
        TAG_U8 => {
            let b = payload.first()?;
            Some(RedbxValue::U8(*b))
        }
        TAG_U16 => {
            let arr: [u8; 2] = payload.try_into().ok()?;
            Some(RedbxValue::U16(u16::from_le_bytes(arr)))
        }
        TAG_U32 => {
            let arr: [u8; 4] = payload.try_into().ok()?;
            Some(RedbxValue::U32(u32::from_le_bytes(arr)))
        }
        TAG_U64 => {
            let arr: [u8; 8] = payload.try_into().ok()?;
            Some(RedbxValue::U64(u64::from_le_bytes(arr)))
        }
        TAG_I8 => {
            let b = payload.first()?;
            Some(RedbxValue::I8(*b as i8))
        }
        TAG_I16 => {
            let arr: [u8; 2] = payload.try_into().ok()?;
            Some(RedbxValue::I16(i16::from_le_bytes(arr)))
        }
        TAG_I32 => {
            let arr: [u8; 4] = payload.try_into().ok()?;
            Some(RedbxValue::I32(i32::from_le_bytes(arr)))
        }
        TAG_I64 => {
            let arr: [u8; 8] = payload.try_into().ok()?;
            Some(RedbxValue::I64(i64::from_le_bytes(arr)))
        }
        TAG_F32 => {
            let arr: [u8; 4] = payload.try_into().ok()?;
            Some(RedbxValue::F32(f32::from_le_bytes(arr)))
        }
        TAG_F64 => {
            let arr: [u8; 8] = payload.try_into().ok()?;
            Some(RedbxValue::F64(f64::from_le_bytes(arr)))
        }
        TAG_BOOL => {
            let b = payload.first()?;
            Some(RedbxValue::Bool(*b != 0))
        }
        _ => None,
    }
}

// ── Key-value pair ────────────────────────────────────────────────────────────

/// A key-value pair returned from range queries.
#[derive(uniffi::Record, Debug, Clone)]
pub struct RedbxKeyValue {
    pub key: RedbxValue,
    pub value: RedbxValue,
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors returned by redbx mobile operations.
///
/// Collapses redbx's multi-level error hierarchy into a flat enum for FFI ergonomics.
/// The `message` field carries diagnostic detail for all structural variants.
#[derive(uniffi::Error, Debug)]
pub enum RedbxError {
    IncorrectPassword,
    DatabaseAlreadyOpen,
    DatabaseCorrupted { message: String },
    EncryptionFailed { message: String },
    DecryptionFailed { message: String },
    TableTypeMismatch { message: String },
    TableDoesNotExist { message: String },
    TableAlreadyOpen { message: String },
    TransactionConsumed,
    IoError { message: String },
    UnknownError { message: String },
}

impl fmt::Display for RedbxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncorrectPassword => write!(f, "incorrect password"),
            Self::DatabaseAlreadyOpen => write!(f, "database already open"),
            Self::DatabaseCorrupted { message } => write!(f, "database corrupted: {message}"),
            Self::EncryptionFailed { message } => write!(f, "encryption failed: {message}"),
            Self::DecryptionFailed { message } => write!(f, "decryption failed: {message}"),
            Self::TableTypeMismatch { message } => write!(f, "table type mismatch: {message}"),
            Self::TableDoesNotExist { message } => write!(f, "table does not exist: {message}"),
            Self::TableAlreadyOpen { message } => write!(f, "table already open: {message}"),
            Self::TransactionConsumed => {
                write!(f, "transaction already committed or aborted")
            }
            Self::IoError { message } => write!(f, "I/O error: {message}"),
            Self::UnknownError { message } => write!(f, "unknown error: {message}"),
        }
    }
}

impl std::error::Error for RedbxError {}

/// Convert any redbx [`redbx::Error`] into a [`RedbxError`].
impl From<redbx::Error> for RedbxError {
    fn from(e: redbx::Error) -> Self {
        use redbx::Error;
        match e {
            Error::DatabaseAlreadyOpen => Self::DatabaseAlreadyOpen,
            Error::Io(io) => Self::IoError { message: io.to_string() },
            Error::Corrupted(msg) => Self::DatabaseCorrupted { message: msg },
            Error::TableTypeMismatch { table, .. } => {
                Self::TableTypeMismatch { message: table }
            }
            Error::TableIsMultimap(name) | Error::TableIsNotMultimap(name) => {
                Self::TableTypeMismatch { message: name }
            }
            Error::TableDoesNotExist(name) => Self::TableDoesNotExist { message: name },
            other => Self::UnknownError { message: other.to_string() },
        }
    }
}

impl From<redbx::DatabaseError> for RedbxError {
    fn from(e: redbx::DatabaseError) -> Self {
        use redbx::DatabaseError;
        match e {
            DatabaseError::IncorrectPassword => Self::IncorrectPassword,
            DatabaseError::DatabaseAlreadyOpen => Self::DatabaseAlreadyOpen,
            DatabaseError::EncryptionFailed(msg) => Self::EncryptionFailed { message: msg },
            DatabaseError::DecryptionFailed(msg) => Self::DecryptionFailed { message: msg },
            DatabaseError::CorruptedEncryption(msg) => Self::DatabaseCorrupted { message: msg },
            DatabaseError::Storage(s) => Self::from_storage(s),
            other => Self::UnknownError { message: other.to_string() },
        }
    }
}

impl From<redbx::TableError> for RedbxError {
    fn from(e: redbx::TableError) -> Self {
        use redbx::TableError;
        match e {
            TableError::TableTypeMismatch { table, .. } => {
                Self::TableTypeMismatch { message: table }
            }
            TableError::TableIsMultimap(name) | TableError::TableIsNotMultimap(name) => {
                Self::TableTypeMismatch { message: name }
            }
            TableError::TableDoesNotExist(name) => Self::TableDoesNotExist { message: name },
            TableError::TableAlreadyOpen(name, _) => Self::TableAlreadyOpen { message: name },
            TableError::Storage(s) => Self::from_storage(s),
            other => Self::UnknownError { message: other.to_string() },
        }
    }
}

impl From<redbx::StorageError> for RedbxError {
    fn from(e: redbx::StorageError) -> Self {
        Self::from_storage(e)
    }
}

impl RedbxError {
    fn from_storage(e: redbx::StorageError) -> Self {
        use redbx::StorageError;
        match e {
            StorageError::Io(io) => Self::IoError { message: io.to_string() },
            StorageError::Corrupted(msg) => Self::DatabaseCorrupted { message: msg },
            StorageError::DatabaseClosed => {
                Self::UnknownError { message: "database closed".to_string() }
            }
            other => Self::UnknownError { message: other.to_string() },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: RedbxValue) {
        let bytes = value_to_bytes(&v);
        let got = bytes_to_value(&bytes).expect("deserialize failed");
        assert_eq!(v, got);
    }

    #[test]
    fn test_value_roundtrip_all_types() {
        roundtrip(RedbxValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));
        roundtrip(RedbxValue::Bytes(vec![]));
        roundtrip(RedbxValue::Str("hello world".to_string()));
        roundtrip(RedbxValue::Str(String::new()));
        roundtrip(RedbxValue::U8(255));
        roundtrip(RedbxValue::U16(65535));
        roundtrip(RedbxValue::U32(u32::MAX));
        roundtrip(RedbxValue::U64(u64::MAX));
        roundtrip(RedbxValue::I8(-128));
        roundtrip(RedbxValue::I16(i16::MIN));
        roundtrip(RedbxValue::I32(i32::MIN));
        roundtrip(RedbxValue::I64(i64::MIN));
        roundtrip(RedbxValue::F32(std::f32::consts::PI));
        roundtrip(RedbxValue::F64(std::f64::consts::E));
        roundtrip(RedbxValue::Bool(true));
        roundtrip(RedbxValue::Bool(false));
    }

    #[test]
    fn test_bytes_to_value_empty_returns_none() {
        assert!(bytes_to_value(&[]).is_none());
    }

    #[test]
    fn test_bytes_to_value_unknown_tag_returns_none() {
        assert!(bytes_to_value(&[0xFF, 0x00]).is_none());
    }
}
