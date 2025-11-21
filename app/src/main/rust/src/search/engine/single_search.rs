use super::super::types::{SearchValue, ValueType};
use super::manager::{BPLUS_TREE_ORDER, PAGE_MASK, PAGE_SIZE, ValuePair};
use crate::core::DRIVER_MANAGER;
use crate::wuwa::PageStatusBitmap;
use anyhow::{Result, anyhow};
use bplustree::BPlusTreeSet;
use log::{Level, debug, log_enabled, warn};

pub(crate) fn search_region(
    target: &SearchValue,
    start: u64,        // 区域起始地址
    end: u64,          // 区域结束地址
    chunk_size: usize, // 每次读取的块大小
) -> Result<BPlusTreeSet<ValuePair>> {
    let driver_manager = DRIVER_MANAGER
        .read()
        .map_err(|_| anyhow!("Failed to acquire DriverManager lock"))?;

    let value_type = target.value_type();
    let element_size = value_type.size();

    let mut results = BPlusTreeSet::new(BPLUS_TREE_ORDER);
    let mut read_success = 0usize;
    let mut read_failed = 0usize;
    let mut matches_checked = 0usize;

    let mut current = start & !(*PAGE_SIZE as u64 - 1); // 当前的页对齐地址
    let mut chunk_buffer = vec![0u8; chunk_size]; // 读取缓冲区

    while current < end {
        let chunk_end = (current + chunk_size as u64).min(end); // 当前块的结束地址，如果超过end则取end
        let chunk_len = (chunk_end - current) as usize; // 当前块的实际长度

        let mut page_status = PageStatusBitmap::new(chunk_len, current as usize);

        let read_result = driver_manager.read_memory_unified(
            current, 
            &mut chunk_buffer[..chunk_len], 
            Some(&mut page_status)
        );

        match read_result {
            Ok(_) => {
                let success_pages = page_status.success_count();
                if success_pages > 0 {
                    read_success += 1;
                    search_in_buffer_with_status(
                        &chunk_buffer[..chunk_len],
                        current,
                        start,
                        end,
                        element_size,
                        target,
                        value_type,
                        &page_status,
                        &mut results,
                        &mut matches_checked,
                    );
                } else {
                    read_failed += 1;
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
            },
        }

        current = chunk_end;
    }

    if log_enabled!(Level::Debug) {
        let region_size = end - start;
        debug!(
            "Region stats: size={}MB, reads={} success + {} failed, matches_checked={}, found={}",
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
pub(crate) fn search_in_buffer_with_status(
    buffer: &[u8],
    buffer_addr: u64,                      // 当前读取的缓冲区对应目标进程的一块内存的起始地址
    region_start: u64,                     // 搜索区域的起始地址
    region_end: u64,                       // 搜索区域的结束地址
    element_size: usize,                   // 元素大小
    target: &SearchValue,                  // 目标搜索值
    value_type: ValueType,                 // 目标值类型
    page_status: &PageStatusBitmap,        // 页面状态位图
    results: &mut BPlusTreeSet<ValuePair>, // 搜索结果
    matches_checked: &mut usize,           // 检查的匹配数
) {
    let buffer_end = buffer_addr + buffer.len() as u64; // 结束地址
    let search_start = buffer_addr.max(region_start); // 实际搜索起始地址
    let search_end = buffer_end.min(region_end); // 实际搜索结束地址

    // 避免用户输入非对齐的地址，确保从对齐地址开始搜索
    let rem = search_start % element_size as u64; // 对齐调整
    let first_addr = if rem == 0 {
        search_start // 已对齐
    } else {
        search_start + element_size as u64 - rem // 调整到下一个对齐地址
    };

    // 使用内核风格的页索引追踪：遇到新页时递增
    let mut page_index = 0usize;
    let mut last_page_va = u64::MAX; // 初始化为无效值

    let mut addr = first_addr; // 从第一个对齐地址开始搜索
    while addr + element_size as u64 <= search_end {
        // 遍历搜索范围，只要还有足够的空间存放一个元素就继续
        let offset = (addr - buffer_addr) as usize; // 计算当前地址在缓冲区中的偏移
        if offset + element_size <= buffer.len() {
            // 确保偏移加元素大小不超过缓冲区长度，二次检查

            // 计算当前地址所在的页
            let page_va = addr & *PAGE_MASK as u64;

            // 如果移动到新页，递增页索引（类似内核实现）
            if page_va != last_page_va && last_page_va != u64::MAX {
                page_index += 1;
            }
            last_page_va = page_va;

            // 检查该页是否成功读取，如果成功则进行匹配检查
            if page_index < page_status.num_pages() && page_status.is_page_success(page_index) {
                // 获取当前元素的字节切片
                let element_bytes = &buffer[offset..offset + element_size];
                *matches_checked += 1;

                // 检查是否匹配目标值
                if let Ok(true) = target.matched(element_bytes) {
                    results.insert((addr, value_type).into());
                }
            }
        }
        // 移动到下一个对齐地址
        addr += element_size as u64;
    }
}
