use redbx::{Database, ReadableDatabase, TableDefinition};
use tempfile::NamedTempFile;

const TEST_TABLE: TableDefinition<u32, &str> = TableDefinition::new("test");

#[test]
fn test_windows_database_operations() {
    // Create a temporary file for testing
    let temp_file = NamedTempFile::new().unwrap();
    let db_path = temp_file.path();
    
    // Test database creation
    let db = Database::create(db_path, "test_password").unwrap();
    
    // Test write transaction
    let write_txn = db.begin_write().unwrap();
    {
        let mut table = write_txn.open_table(TEST_TABLE).unwrap();
        table.insert(&1u32, &"hello").unwrap();
        table.insert(&2u32, &"world").unwrap();
    }
    write_txn.commit().unwrap();
    
    // Test read transaction
    let read_txn = db.begin_read().unwrap();
    {
        let table = read_txn.open_table(TEST_TABLE).unwrap();
        let value1 = table.get(&1u32).unwrap();
        let value2 = table.get(&2u32).unwrap();
        
        assert_eq!(value1.unwrap().value(), "hello");
        assert_eq!(value2.unwrap().value(), "world");
    }
    drop(read_txn);
    drop(db);
    
    // Test reopening database
    let db2 = Database::open(db_path, "test_password").unwrap();
    let read_txn2 = db2.begin_read().unwrap();
    {
        let table = read_txn2.open_table(TEST_TABLE).unwrap();
        let value1 = table.get(&1u32).unwrap();
        assert_eq!(value1.unwrap().value(), "hello");
    }
}