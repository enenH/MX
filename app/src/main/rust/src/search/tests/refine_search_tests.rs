//! Refine search (filter) tests
//! Tests various combinations of initial search and refine operations

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use bplustree::BPlusTreeSet;
    use crate::search::{
        SearchEngineManager, SearchQuery, SearchValue, ValueType, SearchMode, BPLUS_TREE_ORDER,
    };
    use crate::search::engine::ValuePair;
    use crate::search::tests::mock_memory::MockMemory;
    use crate::wuwa::PageStatusBitmap;

    /// Helper function to perform a single value search
    fn perform_single_search(
        mem: &MockMemory,
        search_value: &SearchValue,
        base_addr: u64,
        region_size: usize,
    ) -> Result<BPlusTreeSet<ValuePair>> {
        let mut results = BPlusTreeSet::new(BPLUS_TREE_ORDER);
        let mut matches_checked = 0usize;

        let chunk_size = 64 * 1024;
        let mut current = base_addr;
        let end_addr = base_addr + region_size as u64;
        let value_type = search_value.value_type();

        while current < end_addr {
            let chunk_end = (current + chunk_size as u64).min(end_addr);
            let chunk_len = (chunk_end - current) as usize;

            let mut chunk_buffer = vec![0u8; chunk_len];
            let mut page_status = PageStatusBitmap::new(chunk_len, current as usize);

            if mem.mem_read_with_status(current, &mut chunk_buffer, &mut page_status).is_ok() {
                SearchEngineManager::search_in_buffer_with_status(
                    &chunk_buffer,
                    current,
                    base_addr,
                    end_addr,
                    value_type.size(),
                    search_value,
                    value_type,
                    &page_status,
                    &mut results,
                    &mut matches_checked,
                );
            }

            current = chunk_end;
        }

        Ok(results)
    }

    /// Helper function to simulate refine search on existing results
    fn refine_results_single(
        mem: &MockMemory,
        existing_results: &BPlusTreeSet<ValuePair>,
        search_value: &SearchValue,
    ) -> Result<BPlusTreeSet<ValuePair>> {
        let mut refined_results = BPlusTreeSet::new(BPLUS_TREE_ORDER);

        for pair in existing_results.iter() {
            let addr = pair.addr;
            let buffer_size = pair.value_type.size();

            if let Ok(buffer) = mem.mem_read(addr, buffer_size) {
                if let Ok(true) = search_value.matched(&buffer) {
                    refined_results.insert(pair.clone());
                }
            }
        }

        Ok(refined_results)
    }

    /// Helper function to simulate refine with group search
    ///
    /// This checks if addresses in the existing result set can form a group pattern.
    /// For example, if previous search found [0x1000, 0x1004, 0x2000] with value 100,
    /// and now 0x1000=200, 0x1004=300, then searching for pattern [200, 300] should
    /// find these two addresses as they form a valid group within range_size.
    fn refine_results_group(
        mem: &MockMemory,
        existing_results: &BPlusTreeSet<ValuePair>,
        query: &SearchQuery,
    ) -> Result<BPlusTreeSet<ValuePair>> {
        let mut refined_results = BPlusTreeSet::new(BPLUS_TREE_ORDER);

        // Read current values at all existing addresses
        // Note: BPlusTreeSet already returns results sorted by address
        let mut addr_values: Vec<(u64, Vec<u8>)> = Vec::new();
        for pair in existing_results.iter() {
            let addr = pair.addr;
            let value_size = pair.value_type.size();

            if let Ok(buffer) = mem.mem_read(addr, value_size) {
                addr_values.push((addr, buffer));
            }
        }

        // For each starting position, try to find a matching group pattern
        for i in 0..addr_values.len() {
            let (start_addr, _) = addr_values[i];
            let max_addr = start_addr + query.range as u64;

            // Try to match the query pattern using addresses within range
            let mut matched_indices = Vec::new();

            for (_query_idx, search_value) in query.values.iter().enumerate() {
                // Find an address in range that matches this search value
                let mut found = false;
                for j in i..addr_values.len() {
                    let (addr, ref value_bytes) = addr_values[j];

                    if addr > max_addr {
                        break; // Out of range
                    }

                    // Skip if this address was already matched
                    if matched_indices.contains(&j) {
                        continue;
                    }

                    // Check if value matches
                    if let Ok(true) = search_value.matched(value_bytes) {
                        matched_indices.push(j);
                        found = true;
                        break;
                    }
                }

                if !found {
                    // This query value couldn't be matched, pattern fails
                    matched_indices.clear();
                    break;
                }
            }

            // If we matched all values in the pattern, add them to results
            if matched_indices.len() == query.values.len() {
                // For ordered mode, verify addresses are in increasing order
                if query.mode == SearchMode::Ordered {
                    let mut is_ordered = true;
                    for k in 1..matched_indices.len() {
                        if addr_values[matched_indices[k]].0 <= addr_values[matched_indices[k-1]].0 {
                            is_ordered = false;
                            break;
                        }
                    }
                    if !is_ordered {
                        continue;
                    }
                }

                // Add all matched addresses to results
                for (query_idx, &addr_idx) in matched_indices.iter().enumerate() {
                    let (addr, _) = addr_values[addr_idx];
                    let value_type = query.values[query_idx].value_type();
                    refined_results.insert(ValuePair::new(addr, value_type));
                }
            }
        }

        Ok(refined_results)
    }

    #[test]
    fn test_single_to_single_refine() -> Result<()> {
        println!("\n=== Test: Single value search → Single value refine ===\n");

        let mut mem = MockMemory::new();
        let base_addr = mem.malloc(0x7000000000, 1 * 1024 * 1024)?; // 1MB

        // Write test data: value1 at offset, value2 at offset+4
        let test_data = vec![
            (0x1000, 100u32, 200u32),  // First: ✓, Refine: ✓
            (0x2000, 100u32, 300u32),  // First: ✓, Refine: ✗
            (0x3000, 100u32, 200u32),  // First: ✓, Refine: ✓
            (0x4000, 150u32, 200u32),  // First: ✗
            (0x5000, 100u32, 200u32),  // First: ✓, Refine: ✓
            (0x6000, 100u32, 250u32),  // First: ✓, Refine: ✗
        ];

        for (offset, val1, val2) in &test_data {
            mem.mem_write_u32(base_addr + offset, *val1)?;
            mem.mem_write_u32(base_addr + offset + 4, *val2)?;
            println!("Write: 0x{:X} = {}, +4 = {}", base_addr + offset, val1, val2);
        }

        // First search: Find all addresses with value 100
        let query1 = SearchValue::fixed(100, ValueType::Dword);
        let results1 = perform_single_search(&mem, &query1, base_addr, 1 * 1024 * 1024)?;

        results1.iter().for_each(|pair| {
            println!("Found: 0x{:X}", pair.addr);
        });

        println!("\nFirst search results: {} matches for value 100", results1.len());
        assert_eq!(results1.len(), 5, "Should find 5 addresses with value 100");

        // Modify some values in memory (simulating value changes)
        mem.mem_write_u32(base_addr + 0x2000, 200u32)?;
        mem.mem_write_u32(base_addr + 0x6000, 200u32)?;

        // Refine search
        let query2 = SearchValue::fixed(200, ValueType::Dword);
        let results2 = refine_results_single(&mem, &results1, &query2)?;

        println!("\nRefine search results: {} addresses have value 200", results2.len());
        assert_eq!(results2.len(), 2, "Should find 2 addresses that have value 200");

        results2.iter().for_each(|pair| {
            println!("Found: 0x{:X}", pair.addr);
        });

        println!("\nTest completed!");
        Ok(())
    }

    #[test]
    fn test_single_to_group_refine() -> Result<()> {
        println!("\n=== Test: Single value search → Group search refine ===\n");

        let mut mem = MockMemory::new();
        let base_addr = mem.malloc(0x7100000000, 1 * 1024 * 1024)?;

        // Write test data: single value, then check if followed by a pattern
        let test_patterns = vec![
            (0x1000, 100u32, 200u32, 300u32),  // Match: 100, has [200,300] nearby
            (0x2000, 100u32, 250u32, 300u32),  // Match: 100, no [200,300]
            (0x3000, 100u32, 200u32, 300u32),  // Match: 100, has [200,300] nearby
            (0x4000, 150u32, 200u32, 300u32),  // No initial match
            (0x5000, 100u32, 200u32, 350u32),  // Match: 100, no [200,300]
        ];

        for (offset, v1, v2, v3) in &test_patterns {
            mem.mem_write_u32(base_addr + offset, *v1)?;
            mem.mem_write_u32(base_addr + offset + 4, *v2)?;
            mem.mem_write_u32(base_addr + offset + 8, *v3)?;
            println!("Write: 0x{:X} = [{}, {}, {}]", base_addr + offset, v1, v2, v3);
        }
        let query1 = SearchValue::fixed(100, ValueType::Dword);
        let results1 = perform_single_search(&mem, &query1, base_addr, 1 * 1024 * 1024)?;

        println!("\nFirst search results: {} matches for value 100", results1.len());
        assert_eq!(results1.len(), 4, "Should find 4 addresses with value 100");

        results1.iter().for_each(|pair| {
            println!("Found: 0x{:X}", pair.addr);
        });

        let query2 = SearchQuery::new(
            vec![
                SearchValue::fixed(200, ValueType::Dword),
                SearchValue::fixed(300, ValueType::Dword),
            ],
            SearchMode::Ordered,
            128,
        );

        let results2 = refine_results_group(&mem, &results1, &query2)?;

        println!("\nRefine search results: {} addresses have pattern [200, 300] nearby", results2.len());

        results2.iter().for_each(|pair| {
            println!("Found: 0x{:X}", pair.addr);
        });

        assert_eq!(results2.len(), 0, "Should find 0 addresses with pattern [200, 300]");

        println!("\nTest completed!");
        Ok(())
    }

    #[test]
    fn test_group_to_single_refine() -> Result<()> {
        println!("\n=== Test: Group search → Single value refine ===\n");

        let mut mem = MockMemory::new();
        let base_addr = mem.malloc(0x7200000000, 1 * 1024 * 1024)?;

        // Write test patterns
        let test_patterns = vec![
            (0x1000, 100u32, 200u32),  // Has pattern [100,200]
            (0x2000, 100u32, 200u32),  // Has pattern [100,200]
            (0x3000, 100u32, 200u32),  // Has pattern [100,200]
            (0x4000, 100u32, 250u32),  // No pattern match
            (0x5000, 100u32, 200u32),  // Has pattern [100,200]
        ];

        for (offset, v1, v2) in &test_patterns {
            mem.mem_write_u32(base_addr + offset, *v1)?;
            mem.mem_write_u32(base_addr + offset + 4, *v2)?;
            println!("Write: 0x{:X} = [{}, {}]", base_addr + offset, v1, v2);
        }

        // First search: Find pattern [100, 200]
        let query1 = SearchQuery::new(
            vec![
                SearchValue::fixed(100, ValueType::Dword),
                SearchValue::fixed(200, ValueType::Dword),
            ],
            SearchMode::Ordered,
            128,
        );

        // Perform group search manually
        let mut results1 = BPlusTreeSet::new(BPLUS_TREE_ORDER);
        let chunk_size = 64 * 1024;
        let mut current = base_addr;
        let end_addr = base_addr + 1 * 1024 * 1024;

        while current < end_addr {
            let chunk_end = (current + chunk_size as u64).min(end_addr);
            let chunk_len = (chunk_end - current) as usize;

            let mut chunk_buffer = vec![0u8; chunk_len];
            let mut page_status = PageStatusBitmap::new(chunk_len, current as usize);

            if mem.mem_read_with_status(current, &mut chunk_buffer, &mut page_status).is_ok() {
                // Scan for group pattern
                for offset in (0..chunk_len.saturating_sub(query1.total_size() + query1.range as usize)).step_by(4) {
                    let addr = current + offset as u64;
                    let slice_end = (offset + query1.total_size() + query1.range as usize).min(chunk_len);
                    let slice = &chunk_buffer[offset..slice_end];

                    if let Some(value_offsets) = SearchEngineManager::try_match_group_at_address(slice, addr, &query1) {
                        for (idx, value_offset) in value_offsets.iter().enumerate() {
                            let value_addr = addr + *value_offset as u64;
                            let value_type = query1.values[idx].value_type();
                            results1.insert(ValuePair::new(value_addr, value_type));
                        }
                    }
                }
            }

            current = chunk_end;
        }

        println!("\nFirst search results: {} addresses in patterns [100, 200]", results1.len());

        // Modify one value
        mem.mem_write_u32(base_addr + 0x2000, 150u32)?; // Change 100 to 150

        // Refine with single value: Keep only addresses that still have value 100
        let query2 = SearchValue::fixed(100, ValueType::Dword);
        let results2 = refine_results_single(&mem, &results1, &query2)?;

        println!("\nRefine search results: {} addresses still have value 100", results2.len());

        println!("\nTest completed!");
        Ok(())
    }

    #[test]
    fn test_group_to_group_refine() -> Result<()> {
        println!("\n=== Test: Group search → Group search refine ===\n");

        let mut mem = MockMemory::new();
        let base_addr = mem.malloc(0x7300000000, 1 * 1024 * 1024)?;

        // Write complex patterns
        let test_patterns = vec![
            (0x1000, 100u32, 200u32, 300u32, 400u32),  // Has [100,200] and [300,400]
            (0x2000, 100u32, 200u32, 350u32, 400u32),  // Has [100,200] but not [300,400]
            (0x3000, 100u32, 200u32, 300u32, 400u32),  // Has [100,200] and [300,400]
            (0x4000, 150u32, 200u32, 300u32, 400u32),  // No [100,200]
            (0x5000, 100u32, 200u32, 300u32, 400u32),  // Has [100,200] and [300,400]
        ];

        for (offset, v1, v2, v3, v4) in &test_patterns {
            mem.mem_write_u32(base_addr + offset, *v1)?;
            mem.mem_write_u32(base_addr + offset + 4, *v2)?;
            mem.mem_write_u32(base_addr + offset + 8, *v3)?;
            mem.mem_write_u32(base_addr + offset + 12, *v4)?;
            println!("Write: 0x{:X} = [{}, {}, {}, {}]", base_addr + offset, v1, v2, v3, v4);
        }

        // First search: Find pattern [100, 200]
        let query1 = SearchQuery::new(
            vec![
                SearchValue::fixed(100, ValueType::Dword),
                SearchValue::fixed(200, ValueType::Dword),
            ],
            SearchMode::Ordered,
            128,
        );

        // Perform first group search
        let mut results1 = BPlusTreeSet::new(BPLUS_TREE_ORDER);
        let chunk_size = 64 * 1024;
        let mut current = base_addr;
        let end_addr = base_addr + 1 * 1024 * 1024;

        while current < end_addr {
            let chunk_end = (current + chunk_size as u64).min(end_addr);
            let chunk_len = (chunk_end - current) as usize;

            let mut chunk_buffer = vec![0u8; chunk_len];
            let mut page_status = PageStatusBitmap::new(chunk_len, current as usize);

            if mem.mem_read_with_status(current, &mut chunk_buffer, &mut page_status).is_ok() {
                for offset in (0..chunk_len.saturating_sub(query1.total_size() + query1.range as usize)).step_by(4) {
                    let addr = current + offset as u64;
                    let slice_end = (offset + query1.total_size() + query1.range as usize).min(chunk_len);
                    let slice = &chunk_buffer[offset..slice_end];

                    if let Some(value_offsets) = SearchEngineManager::try_match_group_at_address(slice, addr, &query1) {
                        for (idx, value_offset) in value_offsets.iter().enumerate() {
                            let value_addr = addr + *value_offset as u64;
                            let value_type = query1.values[idx].value_type();
                            results1.insert(ValuePair::new(value_addr, value_type));
                        }
                    }
                }
            }

            current = chunk_end;
        }

        println!("\nFirst search results: {} addresses with pattern [100, 200]", results1.len());

        // Refine with another group pattern: [300, 400]
        let query2 = SearchQuery::new(
            vec![
                SearchValue::fixed(300, ValueType::Dword),
                SearchValue::fixed(400, ValueType::Dword),
            ],
            SearchMode::Ordered,
            128,
        );

        let results2 = refine_results_group(&mem, &results1, &query2)?;

        println!("\nRefine search results: {} addresses also have pattern [300, 400]", results2.len());

        println!("\nTest completed!");
        Ok(())
    }

    #[test]
    fn test_range_to_fixed_refine() -> Result<()> {
        println!("\n=== Test: Range search → Fixed value refine ===\n");

        let mut mem = MockMemory::new();
        let base_addr = mem.malloc(0x7400000000, 1 * 1024 * 1024)?;

        // Write test data with various values
        let test_data = vec![
            (0x1000, 75u32),   // In range [50,150]
            (0x2000, 100u32),  // In range [50,150], equals 100
            (0x3000, 125u32),  // In range [50,150]
            (0x4000, 100u32),  // In range [50,150], equals 100
            (0x5000, 50u32),   // In range [50,150]
            (0x6000, 100u32),  // In range [50,150], equals 100
            (0x7000, 200u32),  // Not in range
        ];

        for (offset, val) in &test_data {
            mem.mem_write_u32(base_addr + offset, *val)?;
            println!("Write: 0x{:X} = {}", base_addr + offset, val);
        }

        // First search: Find all values in range [50, 150]
        let query1 = SearchValue::range(50, 150, ValueType::Dword, false);
        let results1 = perform_single_search(&mem, &query1, base_addr, 1 * 1024 * 1024)?;

        println!("\nFirst search results: {} values in range [50, 150]", results1.len());
        assert_eq!(results1.len(), 6, "Should find 6 values in range");

        // Refine search: Keep only addresses with exact value 100
        let query2 = SearchValue::fixed(100, ValueType::Dword);
        let results2 = refine_results_single(&mem, &results1, &query2)?;

        println!("\nRefine search results: {} addresses with value 100", results2.len());
        assert_eq!(results2.len(), 3, "Should find 3 addresses with value 100");

        println!("\nTest completed!");
        Ok(())
    }
}
