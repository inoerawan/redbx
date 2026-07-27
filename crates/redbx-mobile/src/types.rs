use std::fmt;

// ── Value type ────────────────────────────────────────────────────────────────

/// A typed value that can be stored as a key or value in redbx tables.
///
/// # Wire format
///
/// Serialized as `[1-byte tag][order-preserving big-endian payload]`.
///
/// Keys are stored as `&[u8]` and redbx orders them by **lexicographic byte
/// comparison**, so the payload encoding is chosen so that byte order matches
/// the natural order of the Rust value:
///
/// * unsigned integers — plain big-endian
/// * signed integers   — big-endian with the sign bit flipped, so negatives
///   sort before non-negatives
/// * floats            — IEEE-754 `totalOrder`: positives get the sign bit set,
///   negatives are bit-inverted (so `-0.0` sorts just below `+0.0`)
/// * strings / bytes   — raw UTF-8 / raw bytes, already lexicographic
///
/// # Cross-variant ranges
///
/// Because the tag comes first, values of different variants occupy disjoint
/// key ranges — `U8(5)` and `U64(5)` are *not* adjacent. A range query must use
/// the same variant for both endpoints; mixing them returns
/// [`RedbxError::InvalidRange`].
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

// ── Order-preserving scalar encodings ─────────────────────────────────────────

/// Flip the sign bit so that signed integers sort correctly as unsigned
/// big-endian bytes: `i64::MIN` maps to `0x0000…`, `i64::MAX` to `0xFFFF…`.
macro_rules! signed_codec {
    ($enc:ident, $dec:ident, $signed:ty, $unsigned:ty, $bits:expr) => {
        fn $enc(n: $signed) -> $unsigned {
            n.cast_unsigned() ^ (1 << ($bits - 1))
        }
        fn $dec(u: $unsigned) -> $signed {
            (u ^ (1 << ($bits - 1))).cast_signed()
        }
    };
}

signed_codec!(enc_i8, dec_i8, i8, u8, 8);
signed_codec!(enc_i16, dec_i16, i16, u16, 16);
signed_codec!(enc_i32, dec_i32, i32, u32, 32);
signed_codec!(enc_i64, dec_i64, i64, u64, 64);

/// Map float bits to an unsigned integer whose numeric order matches IEEE-754
/// `totalOrder`. Positives get the sign bit set; negatives are bit-inverted.
macro_rules! float_codec {
    ($enc:ident, $dec:ident, $float:ty, $unsigned:ty, $sign:expr) => {
        fn $enc(f: $float) -> $unsigned {
            let bits = f.to_bits();
            if bits & $sign == 0 {
                bits ^ $sign
            } else {
                !bits
            }
        }
        fn $dec(u: $unsigned) -> $float {
            let bits = if u & $sign == 0 { !u } else { u ^ $sign };
            <$float>::from_bits(bits)
        }
    };
}

float_codec!(enc_f32, dec_f32, f32, u32, 1u32 << 31);
float_codec!(enc_f64, dec_f64, f64, u64, 1u64 << 63);

/// Build a tagged buffer from a big-endian payload.
fn tagged(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(1 + payload.len());
    out.push(tag);
    out.extend_from_slice(payload);
    out
}

/// Serialize a [`RedbxValue`] to tagged, order-preserving bytes for storage.
pub fn value_to_bytes(v: &RedbxValue) -> Vec<u8> {
    match v {
        RedbxValue::Bytes(b) => tagged(TAG_BYTES, b),
        RedbxValue::Str(s) => tagged(TAG_STR, s.as_bytes()),
        RedbxValue::U8(n) => tagged(TAG_U8, &n.to_be_bytes()),
        RedbxValue::U16(n) => tagged(TAG_U16, &n.to_be_bytes()),
        RedbxValue::U32(n) => tagged(TAG_U32, &n.to_be_bytes()),
        RedbxValue::U64(n) => tagged(TAG_U64, &n.to_be_bytes()),
        RedbxValue::I8(n) => tagged(TAG_I8, &enc_i8(*n).to_be_bytes()),
        RedbxValue::I16(n) => tagged(TAG_I16, &enc_i16(*n).to_be_bytes()),
        RedbxValue::I32(n) => tagged(TAG_I32, &enc_i32(*n).to_be_bytes()),
        RedbxValue::I64(n) => tagged(TAG_I64, &enc_i64(*n).to_be_bytes()),
        RedbxValue::F32(f) => tagged(TAG_F32, &enc_f32(*f).to_be_bytes()),
        RedbxValue::F64(f) => tagged(TAG_F64, &enc_f64(*f).to_be_bytes()),
        RedbxValue::Bool(b) => tagged(TAG_BOOL, &[u8::from(*b)]),
    }
}

/// Read a fixed-size big-endian payload, rejecting any trailing bytes.
fn fixed<const N: usize>(payload: &[u8]) -> Option<[u8; N]> {
    payload.try_into().ok()
}

/// Deserialize a [`RedbxValue`] from tagged bytes. Returns `None` on corrupt data.
pub fn bytes_to_value(data: &[u8]) -> Option<RedbxValue> {
    let (&tag, payload) = data.split_first()?;
    match tag {
        TAG_BYTES => Some(RedbxValue::Bytes(payload.to_vec())),
        TAG_STR => Some(RedbxValue::Str(String::from_utf8(payload.to_vec()).ok()?)),
        TAG_U8 => Some(RedbxValue::U8(u8::from_be_bytes(fixed(payload)?))),
        TAG_U16 => Some(RedbxValue::U16(u16::from_be_bytes(fixed(payload)?))),
        TAG_U32 => Some(RedbxValue::U32(u32::from_be_bytes(fixed(payload)?))),
        TAG_U64 => Some(RedbxValue::U64(u64::from_be_bytes(fixed(payload)?))),
        TAG_I8 => Some(RedbxValue::I8(dec_i8(u8::from_be_bytes(fixed(payload)?)))),
        TAG_I16 => Some(RedbxValue::I16(dec_i16(u16::from_be_bytes(fixed(
            payload,
        )?)))),
        TAG_I32 => Some(RedbxValue::I32(dec_i32(u32::from_be_bytes(fixed(
            payload,
        )?)))),
        TAG_I64 => Some(RedbxValue::I64(dec_i64(u64::from_be_bytes(fixed(
            payload,
        )?)))),
        TAG_F32 => Some(RedbxValue::F32(dec_f32(u32::from_be_bytes(fixed(
            payload,
        )?)))),
        TAG_F64 => Some(RedbxValue::F64(dec_f64(u64::from_be_bytes(fixed(
            payload,
        )?)))),
        TAG_BOOL => {
            let [b] = fixed::<1>(payload)?;
            Some(RedbxValue::Bool(b != 0))
        }
        _ => None,
    }
}

/// Decode a stored key or value, turning corrupt bytes into a hard error rather
/// than silently dropping the entry.
pub(crate) fn decode_stored(data: &[u8], what: &str) -> Result<RedbxValue, RedbxError> {
    bytes_to_value(data).ok_or_else(|| RedbxError::DatabaseCorrupted {
        detail: format!("could not decode stored {what} ({} bytes)", data.len()),
    })
}

/// The wire tag for a value, used to reject cross-variant range endpoints.
fn tag_of(v: &RedbxValue) -> u8 {
    match v {
        RedbxValue::Bytes(_) => TAG_BYTES,
        RedbxValue::Str(_) => TAG_STR,
        RedbxValue::U8(_) => TAG_U8,
        RedbxValue::U16(_) => TAG_U16,
        RedbxValue::U32(_) => TAG_U32,
        RedbxValue::U64(_) => TAG_U64,
        RedbxValue::I8(_) => TAG_I8,
        RedbxValue::I16(_) => TAG_I16,
        RedbxValue::I32(_) => TAG_I32,
        RedbxValue::I64(_) => TAG_I64,
        RedbxValue::F32(_) => TAG_F32,
        RedbxValue::F64(_) => TAG_F64,
        RedbxValue::Bool(_) => TAG_BOOL,
    }
}

/// Encode both endpoints of a range, rejecting mismatched variants.
///
/// Different variants live in disjoint key ranges (the tag is the first byte),
/// so a mixed-variant range would silently return nonsense.
pub(crate) fn encode_range(
    start: &RedbxValue,
    end: &RedbxValue,
) -> Result<(Vec<u8>, Vec<u8>), RedbxError> {
    if tag_of(start) != tag_of(end) {
        return Err(RedbxError::InvalidRange {
            detail: format!(
                "range endpoints must be the same RedbxValue variant, got {} and {}",
                variant_name(start),
                variant_name(end)
            ),
        });
    }
    Ok((value_to_bytes(start), value_to_bytes(end)))
}

fn variant_name(v: &RedbxValue) -> &'static str {
    match v {
        RedbxValue::Bytes(_) => "Bytes",
        RedbxValue::Str(_) => "Str",
        RedbxValue::U8(_) => "U8",
        RedbxValue::U16(_) => "U16",
        RedbxValue::U32(_) => "U32",
        RedbxValue::U64(_) => "U64",
        RedbxValue::I8(_) => "I8",
        RedbxValue::I16(_) => "I16",
        RedbxValue::I32(_) => "I32",
        RedbxValue::I64(_) => "I64",
        RedbxValue::F32(_) => "F32",
        RedbxValue::F64(_) => "F64",
        RedbxValue::Bool(_) => "Bool",
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
/// The `detail` field carries diagnostic text for all structural variants. It is
/// deliberately not called `message`: `UniFFI` maps error variants onto Kotlin
/// `Exception` subclasses, and a field named `message` collides with
/// `Throwable.message`, producing bindings that do not compile.
#[derive(uniffi::Error, Debug)]
pub enum RedbxError {
    IncorrectPassword,
    DatabaseAlreadyOpen,
    DatabaseCorrupted { detail: String },
    EncryptionFailed { detail: String },
    DecryptionFailed { detail: String },
    TableTypeMismatch { detail: String },
    TableDoesNotExist { detail: String },
    TableAlreadyOpen { detail: String },
    TransactionConsumed,
    InvalidRange { detail: String },
    IoError { detail: String },
    UnknownError { detail: String },
}

impl fmt::Display for RedbxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncorrectPassword => write!(f, "incorrect password"),
            Self::DatabaseAlreadyOpen => write!(f, "database already open"),
            Self::DatabaseCorrupted { detail } => write!(f, "database corrupted: {detail}"),
            Self::EncryptionFailed { detail } => write!(f, "encryption failed: {detail}"),
            Self::DecryptionFailed { detail } => write!(f, "decryption failed: {detail}"),
            Self::TableTypeMismatch { detail } => write!(f, "table type mismatch: {detail}"),
            Self::TableDoesNotExist { detail } => write!(f, "table does not exist: {detail}"),
            Self::TableAlreadyOpen { detail } => write!(f, "table already open: {detail}"),
            Self::TransactionConsumed => {
                write!(f, "transaction already committed or aborted")
            }
            Self::InvalidRange { detail } => write!(f, "invalid range: {detail}"),
            Self::IoError { detail } => write!(f, "I/O error: {detail}"),
            Self::UnknownError { detail } => write!(f, "unknown error: {detail}"),
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
            Error::Io(io) => Self::IoError {
                detail: io.to_string(),
            },
            Error::Corrupted(msg) => Self::DatabaseCorrupted { detail: msg },
            Error::TableTypeMismatch { table, .. } => Self::TableTypeMismatch { detail: table },
            Error::TableIsMultimap(name) | Error::TableIsNotMultimap(name) => {
                Self::TableTypeMismatch { detail: name }
            }
            Error::TableDoesNotExist(name) => Self::TableDoesNotExist { detail: name },
            other => Self::UnknownError {
                detail: other.to_string(),
            },
        }
    }
}

impl From<redbx::DatabaseError> for RedbxError {
    fn from(e: redbx::DatabaseError) -> Self {
        use redbx::DatabaseError;
        match e {
            DatabaseError::IncorrectPassword => Self::IncorrectPassword,
            DatabaseError::DatabaseAlreadyOpen => Self::DatabaseAlreadyOpen,
            DatabaseError::EncryptionFailed(msg) => Self::EncryptionFailed { detail: msg },
            DatabaseError::DecryptionFailed(msg) => Self::DecryptionFailed { detail: msg },
            DatabaseError::CorruptedEncryption(msg) => Self::DatabaseCorrupted { detail: msg },
            DatabaseError::Storage(s) => Self::from_storage(s),
            other => Self::UnknownError {
                detail: other.to_string(),
            },
        }
    }
}

impl From<redbx::TableError> for RedbxError {
    fn from(e: redbx::TableError) -> Self {
        use redbx::TableError;
        match e {
            TableError::TableTypeMismatch { table, .. } => {
                Self::TableTypeMismatch { detail: table }
            }
            TableError::TableIsMultimap(name) | TableError::TableIsNotMultimap(name) => {
                Self::TableTypeMismatch { detail: name }
            }
            TableError::TableDoesNotExist(name) => Self::TableDoesNotExist { detail: name },
            TableError::TableAlreadyOpen(name, _) => Self::TableAlreadyOpen { detail: name },
            TableError::Storage(s) => Self::from_storage(s),
            other => Self::UnknownError {
                detail: other.to_string(),
            },
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
            StorageError::Io(io) => Self::IoError {
                detail: io.to_string(),
            },
            StorageError::Corrupted(msg) => Self::DatabaseCorrupted { detail: msg },
            StorageError::DatabaseClosed => Self::UnknownError {
                detail: "database closed".to_string(),
            },
            other => Self::UnknownError {
                detail: other.to_string(),
            },
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
        roundtrip(RedbxValue::U8(0));
        roundtrip(RedbxValue::U8(255));
        roundtrip(RedbxValue::U16(65535));
        roundtrip(RedbxValue::U32(u32::MAX));
        roundtrip(RedbxValue::U64(u64::MAX));
        roundtrip(RedbxValue::I8(-128));
        roundtrip(RedbxValue::I8(127));
        roundtrip(RedbxValue::I16(i16::MIN));
        roundtrip(RedbxValue::I16(i16::MAX));
        roundtrip(RedbxValue::I32(i32::MIN));
        roundtrip(RedbxValue::I64(i64::MIN));
        roundtrip(RedbxValue::I64(-1));
        roundtrip(RedbxValue::I64(0));
        roundtrip(RedbxValue::F32(std::f32::consts::PI));
        roundtrip(RedbxValue::F32(-0.0));
        roundtrip(RedbxValue::F32(f32::INFINITY));
        roundtrip(RedbxValue::F64(std::f64::consts::E));
        roundtrip(RedbxValue::F64(f64::NEG_INFINITY));
        roundtrip(RedbxValue::Bool(true));
        roundtrip(RedbxValue::Bool(false));
    }

    #[test]
    fn test_nan_roundtrips_bit_exactly() {
        let bytes = value_to_bytes(&RedbxValue::F64(f64::NAN));
        match bytes_to_value(&bytes) {
            Some(RedbxValue::F64(f)) => assert_eq!(f.to_bits(), f64::NAN.to_bits()),
            other => panic!("expected F64, got {other:?}"),
        }
    }

    #[test]
    fn test_bytes_to_value_empty_returns_none() {
        assert!(bytes_to_value(&[]).is_none());
    }

    #[test]
    fn test_bytes_to_value_unknown_tag_returns_none() {
        assert!(bytes_to_value(&[0xFF, 0x00]).is_none());
    }

    #[test]
    fn test_bytes_to_value_rejects_wrong_payload_length() {
        // U32 needs exactly 4 payload bytes.
        assert!(bytes_to_value(&[TAG_U32, 0, 0, 0]).is_none());
        assert!(bytes_to_value(&[TAG_U32, 0, 0, 0, 0, 0]).is_none());
        // Bool needs exactly 1.
        assert!(bytes_to_value(&[TAG_BOOL]).is_none());
        assert!(bytes_to_value(&[TAG_BOOL, 1, 1]).is_none());
    }

    // ── Ordering: the encoded bytes must sort like the Rust values ────────────

    /// Assert that `values` (given in ascending Rust order) encode to ascending
    /// byte strings.
    fn assert_encoding_is_ordered(values: &[RedbxValue]) {
        let encoded: Vec<Vec<u8>> = values.iter().map(value_to_bytes).collect();
        for w in encoded.windows(2) {
            assert!(
                w[0] < w[1],
                "encoding not order-preserving: {:?} should sort before {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn test_unsigned_encoding_is_ordered() {
        assert_encoding_is_ordered(&[
            RedbxValue::U64(0),
            RedbxValue::U64(1),
            RedbxValue::U64(2),
            RedbxValue::U64(100),
            RedbxValue::U64(255),
            RedbxValue::U64(256),
            RedbxValue::U64(257),
            RedbxValue::U64(1000),
            RedbxValue::U64(70_000),
            RedbxValue::U64(u64::MAX),
        ]);
        assert_encoding_is_ordered(&[RedbxValue::U8(0), RedbxValue::U8(127), RedbxValue::U8(255)]);
        assert_encoding_is_ordered(&[
            RedbxValue::U16(0),
            RedbxValue::U16(255),
            RedbxValue::U16(256),
            RedbxValue::U16(u16::MAX),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::U32(0),
            RedbxValue::U32(65_535),
            RedbxValue::U32(65_536),
            RedbxValue::U32(u32::MAX),
        ]);
    }

    #[test]
    fn test_signed_encoding_is_ordered() {
        assert_encoding_is_ordered(&[
            RedbxValue::I64(i64::MIN),
            RedbxValue::I64(-70_000),
            RedbxValue::I64(-256),
            RedbxValue::I64(-255),
            RedbxValue::I64(-1),
            RedbxValue::I64(0),
            RedbxValue::I64(1),
            RedbxValue::I64(255),
            RedbxValue::I64(256),
            RedbxValue::I64(i64::MAX),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::I8(i8::MIN),
            RedbxValue::I8(-1),
            RedbxValue::I8(0),
            RedbxValue::I8(i8::MAX),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::I16(i16::MIN),
            RedbxValue::I16(-1),
            RedbxValue::I16(0),
            RedbxValue::I16(i16::MAX),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::I32(i32::MIN),
            RedbxValue::I32(-1),
            RedbxValue::I32(0),
            RedbxValue::I32(i32::MAX),
        ]);
    }

    #[test]
    fn test_float_encoding_is_ordered() {
        assert_encoding_is_ordered(&[
            RedbxValue::F64(f64::NEG_INFINITY),
            RedbxValue::F64(-1e300),
            RedbxValue::F64(-1.5),
            RedbxValue::F64(-1.0),
            RedbxValue::F64(-f64::MIN_POSITIVE),
            RedbxValue::F64(-0.0),
            RedbxValue::F64(0.0),
            RedbxValue::F64(f64::MIN_POSITIVE),
            RedbxValue::F64(1.0),
            RedbxValue::F64(1.5),
            RedbxValue::F64(1e300),
            RedbxValue::F64(f64::INFINITY),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::F32(f32::NEG_INFINITY),
            RedbxValue::F32(-1.0),
            RedbxValue::F32(-0.0),
            RedbxValue::F32(0.0),
            RedbxValue::F32(1.0),
            RedbxValue::F32(f32::INFINITY),
        ]);
    }

    #[test]
    fn test_string_and_bytes_encoding_is_ordered() {
        assert_encoding_is_ordered(&[
            RedbxValue::Str(String::new()),
            RedbxValue::Str("a".to_string()),
            RedbxValue::Str("ab".to_string()),
            RedbxValue::Str("b".to_string()),
            RedbxValue::Str("z".to_string()),
        ]);
        assert_encoding_is_ordered(&[
            RedbxValue::Bytes(vec![]),
            RedbxValue::Bytes(vec![0x00]),
            RedbxValue::Bytes(vec![0x00, 0x01]),
            RedbxValue::Bytes(vec![0x01]),
            RedbxValue::Bytes(vec![0xFF]),
        ]);
    }

    #[test]
    fn test_bool_encoding_is_ordered() {
        assert_encoding_is_ordered(&[RedbxValue::Bool(false), RedbxValue::Bool(true)]);
    }

    // ── Range endpoint validation ─────────────────────────────────────────────

    #[test]
    fn test_encode_range_accepts_matching_variants() {
        assert!(encode_range(&RedbxValue::U64(1), &RedbxValue::U64(9)).is_ok());
    }

    #[test]
    fn test_encode_range_rejects_mixed_variants() {
        let err = encode_range(&RedbxValue::U8(1), &RedbxValue::U64(9)).unwrap_err();
        assert!(matches!(err, RedbxError::InvalidRange { .. }), "{err:?}");
    }

    #[test]
    fn test_decode_stored_reports_corruption() {
        let err = decode_stored(&[0xFF, 0x00], "key").unwrap_err();
        assert!(
            matches!(err, RedbxError::DatabaseCorrupted { .. }),
            "{err:?}"
        );
    }
}
