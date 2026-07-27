use redbx_mobile::{RedbxDatabase, RedbxError, RedbxValue};

fn create_tempfile() -> tempfile::NamedTempFile {
    tempfile::NamedTempFile::new().unwrap()
}

// ── Database lifecycle ────────────────────────────────────────────────────────

#[test]
fn test_create_and_reopen() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();

    let db = RedbxDatabase::create(path.clone(), "pass".to_string()).unwrap();
    drop(db);

    let db = RedbxDatabase::open(path.clone(), "pass".to_string()).unwrap();
    drop(db);
}

#[test]
fn test_wrong_password_returns_error() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();

    let db = RedbxDatabase::create(path.clone(), "correct".to_string()).unwrap();
    // Write data so the file has at least one encrypted data page (verify_password needs this).
    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("t".to_string()).unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(1))
        .unwrap();
    txn.commit().unwrap();
    drop(db);

    let result = RedbxDatabase::open(path, "wrong".to_string());
    assert!(matches!(result, Err(RedbxError::IncorrectPassword)));
}

#[test]
fn test_compact_empty_db() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();
    // compact should not error on empty db
    db.compact().unwrap();
}

// ── Write + read roundtrip ────────────────────────────────────────────────────

#[test]
fn test_insert_and_get() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::Str("hello".to_string()), RedbxValue::U64(42))
        .unwrap();
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    let val = rtable.get(RedbxValue::Str("hello".to_string())).unwrap();
    assert_eq!(val, Some(RedbxValue::U64(42)));
}

#[test]
fn test_get_missing_key_returns_none() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    // Seed the table so it exists for the read transaction.
    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::Str("seed".to_string()), RedbxValue::U64(0))
        .unwrap();
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    let val = rtable.get(RedbxValue::Str("missing".to_string())).unwrap();
    assert_eq!(val, None);
}

#[test]
fn test_remove() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::U32(1), RedbxValue::Bool(true))
        .unwrap();
    let removed = table.remove(RedbxValue::U32(1)).unwrap();
    assert_eq!(removed, Some(RedbxValue::Bool(true)));
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    assert_eq!(rtable.get(RedbxValue::U32(1)).unwrap(), None);
}

#[test]
fn test_len_and_is_empty() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    assert!(table.is_empty().unwrap());
    assert_eq!(table.len().unwrap(), 0);

    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(100))
        .unwrap();
    table
        .insert(RedbxValue::U64(2), RedbxValue::U64(200))
        .unwrap();
    assert_eq!(table.len().unwrap(), 2);
    assert!(!table.is_empty().unwrap());
    txn.commit().unwrap();
}

#[test]
fn test_range_query() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    for i in 0u64..10 {
        table
            .insert(RedbxValue::U64(i), RedbxValue::U64(i * 10))
            .unwrap();
    }
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    let entries = rtable
        .range(RedbxValue::U64(3), RedbxValue::U64(6))
        .unwrap();
    assert_eq!(entries.len(), 4); // keys 3, 4, 5, 6
    assert_eq!(entries[0].key, RedbxValue::U64(3));
    assert_eq!(entries[0].value, RedbxValue::U64(30));
    assert_eq!(entries[3].key, RedbxValue::U64(6));
}

/// Regression test: keys used to be encoded little-endian, so byte-lexicographic
/// ordering in redbx did not match numeric ordering. Values below only expose the
/// bug once they cross a byte boundary — a `0..10` range passes either way.
#[test]
fn test_range_query_across_byte_boundaries() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let keys = [1u64, 2, 100, 255, 256, 257, 1000, 70_000, u64::MAX];

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    for k in keys {
        table
            .insert(RedbxValue::U64(k), RedbxValue::U64(k))
            .unwrap();
    }
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();

    // A full scan must come back in ascending numeric order.
    let all = rtable
        .range(RedbxValue::U64(0), RedbxValue::U64(u64::MAX))
        .unwrap();
    let scanned: Vec<u64> = all
        .iter()
        .map(|kv| match kv.key {
            RedbxValue::U64(n) => n,
            ref other => panic!("unexpected key variant: {other:?}"),
        })
        .collect();
    assert_eq!(scanned, keys, "full scan is not in ascending key order");

    // A bounded range must include every key inside it and nothing outside.
    let sub = rtable
        .range(RedbxValue::U64(1), RedbxValue::U64(300))
        .unwrap();
    let in_range: Vec<u64> = sub
        .iter()
        .map(|kv| match kv.key {
            RedbxValue::U64(n) => n,
            ref other => panic!("unexpected key variant: {other:?}"),
        })
        .collect();
    assert_eq!(in_range, vec![1, 2, 100, 255, 256, 257]);
}

#[test]
fn test_range_query_negative_keys_sort_first() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let keys = [i64::MIN, -70_000, -256, -1, 0, 1, 256, i64::MAX];

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    for k in keys {
        table
            .insert(RedbxValue::I64(k), RedbxValue::I64(k))
            .unwrap();
    }
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    let all = rtable
        .range(RedbxValue::I64(i64::MIN), RedbxValue::I64(i64::MAX))
        .unwrap();
    let scanned: Vec<i64> = all
        .iter()
        .map(|kv| match kv.key {
            RedbxValue::I64(n) => n,
            ref other => panic!("unexpected key variant: {other:?}"),
        })
        .collect();
    assert_eq!(scanned, keys);

    let negatives = rtable
        .range(RedbxValue::I64(i64::MIN), RedbxValue::I64(-1))
        .unwrap();
    assert_eq!(negatives.len(), 4);
}

#[test]
fn test_range_rejects_mixed_variant_endpoints() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(1))
        .unwrap();

    let result = table.range(RedbxValue::U8(0), RedbxValue::U64(10));
    assert!(
        matches!(result, Err(RedbxError::InvalidRange { .. })),
        "expected InvalidRange, got {result:?}"
    );
    txn.abort();
}

#[test]
fn test_multimap_range_across_byte_boundaries() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_multimap_table("mm".to_string()).unwrap();
    for k in [1u64, 255, 256, 1000] {
        table
            .insert(RedbxValue::U64(k), RedbxValue::U64(k))
            .unwrap();
    }
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_multimap_table("mm".to_string()).unwrap();
    let entries = rtable
        .range(RedbxValue::U64(1), RedbxValue::U64(256))
        .unwrap();
    let scanned: Vec<u64> = entries
        .iter()
        .map(|kv| match kv.key {
            RedbxValue::U64(n) => n,
            ref other => panic!("unexpected key variant: {other:?}"),
        })
        .collect();
    assert_eq!(scanned, vec![1, 255, 256]);
}

// ── Transaction lifecycle ─────────────────────────────────────────────────────

#[test]
fn test_abort_discards_changes() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    // Seed the table first so it persists and read txn can open it.
    let setup = db.begin_write().unwrap();
    let setup_table = setup.clone().open_table("kv".to_string()).unwrap();
    setup_table
        .insert(RedbxValue::Str("seed".to_string()), RedbxValue::U64(0))
        .unwrap();
    setup.commit().unwrap();

    // Aborted transaction — inserts should not be visible.
    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::Str("key".to_string()), RedbxValue::I64(-1))
        .unwrap();
    txn.abort();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("kv".to_string()).unwrap();
    assert_eq!(
        rtable.get(RedbxValue::Str("key".to_string())).unwrap(),
        None
    );
}

#[test]
fn test_ops_after_commit_return_error() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("kv".to_string()).unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(1))
        .unwrap();
    txn.commit().unwrap();

    // table still holds Arc to the (now consumed) transaction
    let result = table.insert(RedbxValue::U64(2), RedbxValue::U64(2));
    assert!(matches!(result, Err(RedbxError::TransactionConsumed)));
}

// ── All RedbxValue types roundtrip through a table ───────────────────────────

#[test]
fn test_all_value_types_roundtrip() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let values: Vec<RedbxValue> = vec![
        RedbxValue::Bytes(vec![0xDE, 0xAD]),
        RedbxValue::Str("hello".to_string()),
        RedbxValue::U8(255),
        RedbxValue::U16(1000),
        RedbxValue::U32(100_000),
        RedbxValue::U64(u64::MAX),
        RedbxValue::I8(-128),
        RedbxValue::I16(-1000),
        RedbxValue::I32(i32::MIN),
        RedbxValue::I64(i64::MIN),
        RedbxValue::F32(std::f32::consts::PI),
        RedbxValue::F64(std::f64::consts::PI),
        RedbxValue::Bool(true),
        RedbxValue::Bool(false),
    ];

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_table("types".to_string()).unwrap();
    for (i, v) in values.iter().enumerate() {
        table.insert(RedbxValue::U64(i as u64), v.clone()).unwrap();
    }
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_table("types".to_string()).unwrap();
    for (i, expected) in values.iter().enumerate() {
        let got = rtable.get(RedbxValue::U64(i as u64)).unwrap();
        assert_eq!(got.as_ref(), Some(expected), "mismatch at index {i}");
    }
}

// ── Multimap tables ───────────────────────────────────────────────────────────

#[test]
fn test_multimap_insert_and_get() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_multimap_table("tags".to_string()).unwrap();
    table
        .insert(
            RedbxValue::Str("post1".to_string()),
            RedbxValue::Str("rust".to_string()),
        )
        .unwrap();
    table
        .insert(
            RedbxValue::Str("post1".to_string()),
            RedbxValue::Str("database".to_string()),
        )
        .unwrap();
    table
        .insert(
            RedbxValue::Str("post1".to_string()),
            RedbxValue::Str("encrypted".to_string()),
        )
        .unwrap();
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn
        .clone()
        .open_multimap_table("tags".to_string())
        .unwrap();
    let mut values = rtable.get(RedbxValue::Str("post1".to_string())).unwrap();
    values.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    assert_eq!(values.len(), 3);
}

#[test]
fn test_multimap_remove_single() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_multimap_table("mm".to_string()).unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(10))
        .unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(20))
        .unwrap();
    let removed = table
        .remove(RedbxValue::U64(1), RedbxValue::U64(10))
        .unwrap();
    assert!(removed);
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_multimap_table("mm".to_string()).unwrap();
    let values = rtable.get(RedbxValue::U64(1)).unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0], RedbxValue::U64(20));
}

#[test]
fn test_multimap_remove_all() {
    let tmp = create_tempfile();
    let path = tmp.path().to_str().unwrap().to_string();
    let db = RedbxDatabase::create(path, "pass".to_string()).unwrap();

    let txn = db.begin_write().unwrap();
    let table = txn.clone().open_multimap_table("mm".to_string()).unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(10))
        .unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(20))
        .unwrap();
    table
        .insert(RedbxValue::U64(1), RedbxValue::U64(30))
        .unwrap();
    let count = table.remove_all(RedbxValue::U64(1)).unwrap();
    assert_eq!(count, 3);
    txn.commit().unwrap();

    let rtxn = db.begin_read().unwrap();
    let rtable = rtxn.clone().open_multimap_table("mm".to_string()).unwrap();
    let values = rtable.get(RedbxValue::U64(1)).unwrap();
    assert!(values.is_empty());
}
