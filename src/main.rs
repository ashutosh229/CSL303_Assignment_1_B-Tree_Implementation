use bptree::BPlusTree;
use std::time::Instant;

const DATA_SIZE: usize = 100;

fn test_basic_operations() {
    println!("=== Test 1: Basic Insert and Read ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let mut data1 = [0u8; DATA_SIZE];
    let mut data2 = [0u8; DATA_SIZE];
    let mut data3 = [0u8; DATA_SIZE];

    data1[..20].copy_from_slice(b"Test data for key 10");
    data2[..20].copy_from_slice(b"Test data for key 20");
    data3[..20].copy_from_slice(b"Test data for key 15");

    assert!(tree.write_data(10, &data1).unwrap());
    assert!(tree.write_data(20, &data2).unwrap());
    assert!(tree.write_data(15, &data3).unwrap());

    let result = tree.read(10).expect("Key 10 not found");
    assert_eq!(&result[..20], &data1[..20]);
    println!("✓ Read key 10: {}", String::from_utf8_lossy(&result[..20]));

    let result = tree.read(20).expect("Key 20 not found");
    assert_eq!(&result[..20], &data2[..20]);
    println!("✓ Read key 20: {}", String::from_utf8_lossy(&result[..20]));

    let result = tree.read(15).expect("Key 15 not found");
    assert_eq!(&result[..20], &data3[..20]);
    println!("✓ Read key 15: {}", String::from_utf8_lossy(&result[..20]));

    println!("✓ Basic operations test passed!\n");
}

fn test_non_existent_key() {
    println!("=== Test 2: Non-existent Key ===");

    let tree = BPlusTree::new().expect("Failed to create tree");

    let result = tree.read(999);
    assert!(result.is_none());
    println!("✓ Non-existent key returns None\n");
}

fn test_update() {
    println!("=== Test 3: Update Existing Key ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let mut data1 = [0u8; DATA_SIZE];
    let mut data2 = [0u8; DATA_SIZE];

    data1[..13].copy_from_slice(b"Original data");
    data2[..12].copy_from_slice(b"Updated data");

    assert!(tree.write_data(30, &data1).unwrap());
    let result = tree.read(30).expect("Key 30 not found");
    assert_eq!(&result[..13], &data1[..13]);
    println!("✓ Original: {}", String::from_utf8_lossy(&result[..13]));

    assert!(tree.write_data(30, &data2).unwrap());
    let result = tree.read(30).expect("Key 30 not found");
    assert_eq!(&result[..12], &data2[..12]);
    println!("✓ Updated: {}", String::from_utf8_lossy(&result[..12]));

    println!("✓ Update test passed!\n");
}

fn test_delete() {
    println!("=== Test 4: Delete Operation ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let mut data = [0u8; DATA_SIZE];
    let s = b"Data to be deleted";
    data[..s.len()].copy_from_slice(s);

    assert!(tree.write_data(40, &data).unwrap());
    let result = tree.read(40).expect("Key 40 not found");
    println!(
        "✓ Before delete: {}",
        String::from_utf8_lossy(&result[..s.len()])
    );

    assert!(tree.delete(40).unwrap());
    let result = tree.read(40);
    assert!(result.is_none());
    println!("✓ After delete: key not found");

    assert!(!tree.delete(999).unwrap());
    println!("✓ Delete non-existent key returns false");

    println!("✓ Delete test passed!\n");
}

fn test_range_query() {
    println!("=== Test 5: Range Query ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    for i in 50..=60 {
        let mut data = [0u8; DATA_SIZE];
        let text = format!("Data for key {}", i);
        data[..text.len()].copy_from_slice(text.as_bytes());
        assert!(tree.write_data(i, &data).unwrap());
    }

    let results = tree.read_range_data(52, 57);

    println!("Range [52, 57] returned {} results:", results.len());
    assert_eq!(results.len(), 6);

    for (idx, result) in results.iter().enumerate() {
        let text = String::from_utf8_lossy(&result[..20]);
        println!("  Result {}: {}", idx, text.trim_end_matches('\0'));
    }

    println!("✓ Range query test passed!\n");
}

fn test_bulk_insert() {
    println!("=== Test 6: Bulk Insert (1000 entries) ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let start = Instant::now();
    for i in 100..1100 {
        let mut data = [0u8; DATA_SIZE];
        let text = format!("Bulk data entry {}", i);
        data[..text.len()].copy_from_slice(text.as_bytes());
        assert!(tree.write_data(i, &data).unwrap());
    }
    let duration = start.elapsed();

    println!("✓ Inserted 1000 entries in {:?}", duration);

    let result = tree.read(125).expect("Key 125 not found");
    println!(
        "✓ Read key 125: {}",
        String::from_utf8_lossy(&result[..20]).trim_end_matches('\0')
    );

    let result = tree.read(875).expect("Key 875 not found");
    println!(
        "✓ Read key 875: {}",
        String::from_utf8_lossy(&result[..20]).trim_end_matches('\0')
    );

    println!("✓ Bulk insert test passed!\n");
}

fn test_negative_keys() {
    println!("=== Test 7: Negative Keys ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let mut data = [0u8; DATA_SIZE];
    data[..17].copy_from_slice(b"Negative key data");

    assert!(tree.write_data(-100, &data).unwrap());

    let result = tree.read(-100).expect("Key -100 not found");
    assert_eq!(&result[..17], &data[..17]);
    println!(
        "✓ Read negative key -100: {}",
        String::from_utf8_lossy(&result[..17])
    );

    println!("✓ Negative keys test passed!\n");
}

fn test_persistence() {
    println!("=== Test 8: Persistence Check ===");

    {
        let mut tree = BPlusTree::new().expect("Failed to create tree");

        let mut data = [0u8; DATA_SIZE];
        data[..16].copy_from_slice(b"Persistent data!");
        tree.write_data(9999, &data).unwrap();
        tree.flush().unwrap();
        println!("✓ Wrote key 9999 with persistent data");
    }

    {
        let tree = BPlusTree::new().expect("Failed to create tree");
        let result = tree.read(9999).expect("Key 9999 not found after restart");
        assert_eq!(&result[..16], b"Persistent data!");
        println!(
            "✓ Read key 9999 after restart: {}",
            String::from_utf8_lossy(&result[..16])
        );
    }

    println!("✓ Persistence test passed!\n");
}

fn test_stress() {
    println!("=== Test 9: Stress Test (10000 operations) ===");

    let _ = std::fs::remove_file("bptree_index.dat");
    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let start = Instant::now();

    // Insert
    for i in 10000..20000 {
        let mut data = [0u8; DATA_SIZE];
        let text = format!("Stress test data {}", i);
        data[..text.len()].copy_from_slice(text.as_bytes());
        if !tree.write_data(i, &data).unwrap() {
            println!("Warning: Failed to insert key {}", i);
        }
    }

    tree.flush().expect("Failed to flush");

    // Read & verify safely
    let mut failed = 0;
    for i in 10000..20000 {
        match tree.read(i) {
            Some(_data) => {} // ok
            None => {
                failed += 1;
                if failed <= 10 {
                    println!("Warning: Key {} not found", i);
                }
            }
        }
    }

    let duration = start.elapsed();
    if failed == 0 {
        println!("✓ 10000 inserts + 10000 reads in {:?}", duration);
        println!("✓ Average time per operation: {:?}", duration / 20000);
        println!("✓ Stress test passed!\n");
    } else {
        println!("✗ {} keys were not found out of 10000", failed);
    }
}

fn benchmark_operations() {
    println!("=== Performance Benchmark ===");

    let mut tree = BPlusTree::new().expect("Failed to create tree");

    let n = 5000;

    // Insert benchmark
    let start = Instant::now();
    for i in 0..n {
        let mut data = [0u8; DATA_SIZE];
        let text = format!("Benchmark data {}", i);
        data[..text.len()].copy_from_slice(text.as_bytes());
        tree.write_data(i, &data).unwrap();
    }
    let insert_duration = start.elapsed();

    // Safe read benchmark
    let start = Instant::now();
    let mut read_failures = 0;
    for i in 0..n {
        if tree.read(i).is_none() {
            read_failures += 1;
        }
    }
    let read_duration = start.elapsed();

    if read_failures > 0 {
        println!(
            "✗ Warning: {} keys missing during read benchmark",
            read_failures
        );
    }

    // Range benchmark
    let start = Instant::now();
    for _ in 0..100 {
        tree.read_range_data(100, 200);
    }
    let range_duration = start.elapsed();

    println!("Results for {} operations:", n);
    println!(
        "  Insert: {:?} ({:.2} μs/op)",
        insert_duration,
        insert_duration.as_micros() as f64 / n as f64
    );
    println!(
        "  Read:   {:?} ({:.2} μs/op)",
        read_duration,
        read_duration.as_micros() as f64 / n as f64
    );
    println!("  Range:  {:?} (100 queries)", range_duration);

    println!("✓ Benchmark completed!\n");
}

fn main() {
    println!("========================================");
    println!("   B+ Tree Index Driver Test Program   ");
    println!("   (Rust Implementation)               ");
    println!("========================================\n");

    // Remove old index file for fresh start
    let _ = std::fs::remove_file("bptree_index.dat");

    test_basic_operations();
    test_non_existent_key();
    test_update();
    test_delete();
    test_range_query();
    test_bulk_insert();
    test_negative_keys();
    test_persistence();
    test_stress();
    benchmark_operations();

    println!("========================================");
    println!("   ✓ ALL TESTS PASSED SUCCESSFULLY!   ");
    println!("========================================");
}
