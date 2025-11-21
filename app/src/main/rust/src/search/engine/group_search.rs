use super::super::types::{SearchMode, SearchQuery, SearchValue, ValueType};
use super::manager::{BPLUS_TREE_ORDER, PAGE_MASK, PAGE_SIZE, ValuePair};
use crate::core::DRIVER_MANAGER;
use crate::wuwa::PageStatusBitmap;
use anyhow::Result;
use anyhow::anyhow;
use bplustree::BPlusTreeSet;
use log::{Level, debug, log_enabled, warn};
use memchr::memmem;

pub(crate) fn search_region_group(
    query: &SearchQuery,
    start: u64,
    end: u64,
    per_chunk_size: usize,
) -> Result<BPlusTreeSet<ValuePair>> {
    let driver_manager = DRIVER_MANAGER
        .read()
        .map_err(|_| anyhow!("Failed to acquire DriverManager lock"))?;

    let mut results = BPlusTreeSet::new(BPLUS_TREE_ORDER);
    let mut read_success = 0usize;
    let mut read_failed = 0usize;
    let mut matches_checked = 0usize;

    let min_element_size = query.values.iter().map(|v| v.value_type().size()).min().unwrap_or(1);
    let search_range = query.range as usize;

    let mut current = start & *PAGE_MASK as u64;
    let mut sliding_buffer = vec![0u8; per_chunk_size * 2]; // 双倍大小的滑动窗口缓冲区
    let mut is_first_chunk = true; // 是否是第一个chunk
    let mut prev_chunk_valid = false; // 前半部分是否有效（读取成功）

    while current < end {
        let chunk_end = (current + per_chunk_size as u64).min(end);
        let chunk_len = (chunk_end - current) as usize;

        let mut page_status = PageStatusBitmap::new(chunk_len, current as usize);

        // 读取数据到滑动窗口的后半部分
        let read_result = driver_manager.read_memory_unified(
            current, 
            &mut sliding_buffer[per_chunk_size..per_chunk_size + chunk_len], 
            Some(&mut page_status)
        );

        match read_result {
            Ok(_) => {
                let success_pages = page_status.success_count();
                if success_pages > 0 {
                    read_success += 1;

                    if is_first_chunk {
                        // 第一个chunk：只搜索前半部分（刚读取的数据）
                        search_in_buffer_group(
                            &sliding_buffer[per_chunk_size..per_chunk_size + chunk_len],
                            current,
                            start,
                            chunk_end,
                            min_element_size,
                            query,
                            &page_status,
                            &mut results,
                            &mut matches_checked,
                        );
                        is_first_chunk = false;
                    } else if prev_chunk_valid {
                        // 非第一个chunk且前一个chunk有效：搜索重叠区域（从前半部分尾部到后半部分末尾）
                        let overlap_start_offset = per_chunk_size.saturating_sub(search_range);
                        let overlap_start_addr = current - search_range as u64;
                        let overlap_len = search_range + chunk_len;

                        // 创建一个组合的page_status用于重叠区域搜索
                        // 前半部分（重叠部分）假定已成功，后半部分使用实际的page_status
                        let mut combined_status = PageStatusBitmap::new(overlap_len, overlap_start_addr as usize);

                        let overlap_start_page = (overlap_start_addr as usize) / *PAGE_SIZE;
                        let overlap_end = overlap_start_addr as usize + search_range;
                        let overlap_end_page = (overlap_end + *PAGE_SIZE - 1) / *PAGE_SIZE;
                        let num_overlap_pages = overlap_end_page - overlap_start_page;

                        // 标记前半部分（重叠部分）为成功
                        for i in 0..num_overlap_pages {
                            combined_status.mark_success(i);
                        }

                        // 将后半部分的page_status映射到combined_status
                        let page_status_base = (current as usize) & *PAGE_MASK;
                        let combined_base = (overlap_start_addr as usize) & *PAGE_MASK;
                        let page_offset = (page_status_base - combined_base) / *PAGE_SIZE;

                        for i in 0..page_status.num_pages() {
                            if page_status.is_page_success(i) {
                                let combined_page_index = page_offset + i;
                                if combined_page_index < combined_status.num_pages() {
                                    combined_status.mark_success(combined_page_index);
                                }
                            }
                        }

                        search_in_buffer_group(
                            &sliding_buffer[overlap_start_offset..per_chunk_size + chunk_len],
                            overlap_start_addr,
                            start,
                            chunk_end,
                            min_element_size,
                            query,
                            &combined_status,
                            &mut results,
                            &mut matches_checked,
                        );
                    } else {
                        // 前一个chunk无效：只搜索当前chunk（后半部分）
                        search_in_buffer_group(
                            &sliding_buffer[per_chunk_size..per_chunk_size + chunk_len],
                            current,
                            start,
                            chunk_end,
                            min_element_size,
                            query,
                            &page_status,
                            &mut results,
                            &mut matches_checked,
                        );
                    }

                    prev_chunk_valid = true;
                } else {
                    read_failed += 1;
                    prev_chunk_valid = false;
                }
            },
            Err(error) => {
                if log_enabled!(Level::Debug) {
                    warn!(
                        "Failed to read memory at 0x{:X} - 0x{:X}, err: {:?}",
                        current, chunk_end, error
                    );
                }
                read_failed += 1;
                prev_chunk_valid = false;
            },
        }

        // 滑动窗口：把后半部分移动到前半部分
        if chunk_end < end {
            sliding_buffer.copy_within(per_chunk_size..per_chunk_size + chunk_len, 0);
        }

        current = chunk_end;
    }

    if log_enabled!(Level::Debug) {
        let region_size = end - start;
        debug!(
            "Group search stats: size={}MB, reads={} success + {} failed, matches_checked={}, found={}",
            region_size / 1024 / 1024,
            read_success,
            read_failed,
            matches_checked,
            results.len()
        );
    }

    Ok(results)
}

#[inline]
pub(crate) fn search_in_buffer_group(
    buffer: &[u8],
    buffer_addr: u64,
    region_start: u64,
    region_end: u64,
    min_element_size: usize,
    query: &SearchQuery,
    page_status: &PageStatusBitmap,
    results: &mut BPlusTreeSet<ValuePair>,
    matches_checked: &mut usize,
) {
    // anchor-first 优化：尝试使用第一个 Fixed 值作为 anchor 进行 SIMD 扫描
    let mut anchor_index = None;
    let mut anchor_bytes_storage = [0u8; 8]; // 最大 8 字节（Qword/Double）
    let mut anchor_bytes_len = 0;

    for (idx, value) in query.values.iter().enumerate() {
        match value {
            SearchValue::FixedInt { value, value_type } => {
                let size = value_type.size();
                anchor_bytes_storage[..size].copy_from_slice(&value[..size]);
                anchor_bytes_len = size;
                anchor_index = Some(idx);
                break;
            },
            SearchValue::FixedFloat { value, value_type } => {
                match value_type {
                    ValueType::Float => {
                        let f32_val = *value as f32;
                        let bytes = f32_val.to_le_bytes();
                        anchor_bytes_storage[..4].copy_from_slice(&bytes);
                        anchor_bytes_len = 4;
                    },
                    ValueType::Double => {
                        let bytes = value.to_le_bytes();
                        anchor_bytes_storage[..8].copy_from_slice(&bytes);
                        anchor_bytes_len = 8;
                    },
                    _ => continue,
                }
                anchor_index = Some(idx);
                break;
            },
            _ => continue,
        }
    }

    // 如果没有找到 Fixed 值作为 anchor，回退到传统逐地址扫描
    if anchor_index.is_none() {
        search_in_buffer_group_fallback(
            buffer,
            buffer_addr,
            region_start,
            region_end,
            min_element_size,
            query,
            page_status,
            results,
            matches_checked,
        );
        return;
    }

    // 使用 anchor-first SIMD 优化
    let anchor_bytes = &anchor_bytes_storage[..anchor_bytes_len];
    let finder = memmem::Finder::new(anchor_bytes);
    let mut candidates = Vec::new();
    let mut pos = 0;

    let buffer_end = buffer_addr + buffer.len() as u64;
    let search_start = buffer_addr.max(region_start);
    let search_end = buffer_end.min(region_end);

    // 计算第一个对齐地址
    let rem = search_start % min_element_size as u64;
    let first_addr = if rem == 0 {
        search_start
    } else {
        search_start + min_element_size as u64 - rem
    };

    // 预先构建成功页的地址范围
    let page_ranges = page_status.get_success_page_ranges();
    if page_ranges.is_empty() {
        return;
    }

    let buffer_page_start = buffer_addr & !(*PAGE_SIZE as u64 - 1);
    let anchor_alignment = anchor_bytes_len;

    // SIMD 快速扫描找到所有 anchor 候选位置
    while pos < buffer.len() {
        if let Some(offset) = finder.find(&buffer[pos..]) {
            let absolute_offset = pos + offset;
            let addr = buffer_addr + absolute_offset as u64;

            // 过滤1: 检查对齐（使用 anchor 的大小）
            if addr % anchor_alignment as u64 == 0 && addr >= first_addr && addr < search_end {
                candidates.push(absolute_offset);
            }

            pos = absolute_offset + 1;
        } else {
            break;
        }
    }

    // 对候选位置做页面过滤和完整校验
    let anchor_idx = anchor_index.unwrap();

    for &offset in &candidates {
        let anchor_addr = buffer_addr + offset as u64;

        // 根据搜索模式计算需要验证的区域
        let (start_addr, _start_offset) = if query.mode == SearchMode::Ordered {
            // Ordered 模式：根据 anchor 在 query 中的位置，反推序列起始位置
            let anchor_offset_in_sequence = query.values[..anchor_idx]
                .iter()
                .map(|v| v.value_type().size())
                .sum::<usize>();

            let seq_start_addr = anchor_addr.saturating_sub(anchor_offset_in_sequence as u64);
            let seq_start_offset = offset.saturating_sub(anchor_offset_in_sequence);

            (seq_start_addr, seq_start_offset)
        } else {
            // Unordered 模式：从 anchor_addr - range 开始
            let range_start = anchor_addr.saturating_sub(query.range as u64);
            let range_start_offset = if range_start < buffer_addr {
                0
            } else {
                (range_start - buffer_addr) as usize
            };
            (range_start, range_start_offset)
        };

        // 检查地址是否在有效范围内
        let check_range_addr = if query.mode == SearchMode::Ordered {
            start_addr
        } else {
            anchor_addr
        };
        if check_range_addr < region_start || check_range_addr >= region_end {
            continue;
        }

        // 检查地址是否在有效页范围内
        let check_addr = if query.mode == SearchMode::Ordered {
            start_addr
        } else {
            anchor_addr
        };
        let mut in_valid_page = false;
        for (start_page, end_page) in &page_ranges {
            let page_range_start = buffer_page_start + (start_page * *PAGE_SIZE) as u64;
            let page_range_end = buffer_page_start + (end_page * *PAGE_SIZE) as u64;

            if check_addr >= page_range_start && check_addr < page_range_end {
                in_valid_page = true;
                break;
            }
        }

        if !in_valid_page {
            continue;
        }

        // 完整校验
        let total_values_size: usize = query.values.iter().map(|v| v.value_type().size()).sum();
        let min_buffer_size = (total_values_size as u64).max(query.range as u64);

        let (check_start, check_end) = if query.mode == SearchMode::Ordered {
            // Ordered 模式：序列必须完整在 buffer 内才能验证
            if start_addr < buffer_addr {
                continue;
            }
            (
                start_addr,
                (start_addr + min_buffer_size).min(buffer_end).min(region_end),
            )
        } else {
            // Unordered 模式：需要覆盖 anchor 前后的范围
            let unordered_start = anchor_addr.saturating_sub(query.range as u64).max(buffer_addr);
            let unordered_end = (anchor_addr + query.range as u64).min(buffer_end).min(region_end);
            (unordered_start, unordered_end)
        };

        let check_start_offset = (check_start - buffer_addr) as usize;
        let range_size = (check_end - check_start) as usize;

        if check_start_offset + range_size <= buffer.len() {
            *matches_checked += 1;

            if let Some(offsets) = try_match_group_at_address(
                &buffer[check_start_offset..check_start_offset + range_size],
                check_start,
                query,
            ) {
                for (idx, value_offset) in offsets.iter().enumerate() {
                    let value_addr = check_start + *value_offset as u64;
                    let value_type = query.values[idx].value_type();
                    results.insert((value_addr, value_type).into());
                }
            }
        }
    }
}

/// 传统逐地址扫描方法（用于没有 Fixed 值作为 anchor 时的降级）
#[inline]
pub(crate) fn search_in_buffer_group_fallback(
    buffer: &[u8],
    buffer_addr: u64,
    region_start: u64,
    region_end: u64,
    min_element_size: usize,
    query: &SearchQuery,
    page_status: &PageStatusBitmap,
    results: &mut BPlusTreeSet<ValuePair>,
    matches_checked: &mut usize,
) {
    let buffer_end = buffer_addr + buffer.len() as u64;
    let search_start = buffer_addr.max(region_start);
    let search_end = buffer_end.min(region_end);
    let search_range = query.range as u64;

    let rem = search_start % min_element_size as u64;
    let first_addr = if rem == 0 {
        search_start
    } else {
        search_start + min_element_size as u64 - rem
    };

    // 优化：预先构建成功页的地址范围
    let page_ranges = page_status.get_success_page_ranges();
    if page_ranges.is_empty() {
        return;
    }

    // buffer_addr 所在页的起始地址（页对齐）
    let buffer_page_start = buffer_addr & !(*PAGE_SIZE as u64 - 1);

    for (start_page, end_page) in page_ranges {
        // 将相对页索引转换为绝对地址范围
        let page_range_start = buffer_page_start + (start_page * *PAGE_SIZE) as u64;
        let page_range_end = buffer_page_start + (end_page * *PAGE_SIZE) as u64;

        // 限制在 buffer 和搜索范围内
        let range_start = page_range_start.max(buffer_addr);
        let range_end = page_range_end.min(search_end).min(buffer_end);

        if range_start >= range_end {
            continue;
        }

        // 找到这个范围内第一个 >= first_addr 且对齐的地址
        let mut addr = if range_start <= first_addr {
            first_addr // first_addr 已经对齐
        } else {
            // range_start > first_addr，需要对齐
            let rem = range_start % min_element_size as u64;
            if rem == 0 {
                range_start
            } else {
                range_start + min_element_size as u64 - rem
            }
        };

        // 在这个有效页范围内搜索
        while addr < range_end {
            let offset = (addr - buffer_addr) as usize;
            if offset < buffer.len() {
                let range_end_check = (addr + search_range).min(buffer_end).min(search_end);
                let range_size = (range_end_check - addr) as usize;

                if range_size >= query.range as usize && offset + range_size <= buffer.len() {
                    *matches_checked += 1;

                    if let Some(offsets) = try_match_group_at_address(&buffer[offset..offset + range_size], addr, query)
                    {
                        // 保存所有匹配值的地址
                        for (idx, value_offset) in offsets.iter().enumerate() {
                            let value_addr = addr + *value_offset as u64;
                            let value_type = query.values[idx].value_type();
                            results.insert((value_addr, value_type).into());
                        }
                    }
                }
            }
            addr += min_element_size as u64;
        }
    }
}

pub(crate) fn try_match_group_at_address(buffer: &[u8], start_addr: u64, query: &SearchQuery) -> Option<Vec<usize>> {
    match query.mode {
        SearchMode::Ordered => try_match_ordered(buffer, start_addr, query),
        SearchMode::Unordered => try_match_unordered(buffer, start_addr, query),
    }
}

pub(crate) fn try_match_ordered(buffer: &[u8], _start_addr: u64, query: &SearchQuery) -> Option<Vec<usize>> {
    let mut offsets = Vec::with_capacity(query.values.len());
    let mut current_offset = 0usize;

    for target_value in &query.values {
        let value_size = target_value.value_type().size();
        let mut found = false;

        while current_offset + value_size <= buffer.len() {
            let element_bytes = &buffer[current_offset..current_offset + value_size];

            if let Ok(true) = target_value.matched(element_bytes) {
                offsets.push(current_offset);
                current_offset += value_size;
                found = true;
                break;
            }

            let alignment = target_value.value_type().size();
            current_offset += alignment;
        }

        if !found {
            return None;
        }
    }

    Some(offsets)
}

pub(crate) fn try_match_unordered(buffer: &[u8], _start_addr: u64, query: &SearchQuery) -> Option<Vec<usize>> {
    let mut offsets = vec![None; query.values.len()];
    let mut found_count = 0;

    for (value_idx, target_value) in query.values.iter().enumerate() {
        if offsets[value_idx].is_some() {
            continue;
        }

        let value_size = target_value.value_type().size();
        let alignment = value_size;
        let mut offset = 0usize;

        while offset + value_size <= buffer.len() {
            let element_bytes = &buffer[offset..offset + value_size];

            if let Ok(true) = target_value.matched(element_bytes) {
                offsets[value_idx] = Some(offset);
                found_count += 1;
                break;
            }

            offset += alignment;
        }
    }

    if found_count == query.values.len() {
        Some(offsets.into_iter().map(|o| o.unwrap()).collect())
    } else {
        None
    }
}
