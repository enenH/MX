use super::super::SearchResultItem;
use super::super::result_manager::{SearchResultManager, SearchResultMode};
use super::super::types::{SearchQuery, ValueType};
use super::filter::SearchFilter;
use super::group_search;
use super::single_search;
use anyhow::{Result, anyhow};
use bplustree::BPlusTreeSet;
use lazy_static::lazy_static;
use log::{Level, error, log_enabled};
use rayon::prelude::*;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;
use crate::search::result_manager::ExactSearchResultItem;

/// 地址和值类型对
/// 用于存储搜索结果中的地址和值类型信息
/// 实现了 Ord 和 PartialOrd 以便按地址排序
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValuePair {
    pub(crate) addr: u64,
    pub(crate) value_type: ValueType,
}

impl PartialOrd<Self> for ValuePair {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.addr.cmp(&other.addr))
    }
}

impl Ord for ValuePair {
    fn cmp(&self, other: &Self) -> Ordering {
        self.addr.cmp(&other.addr)
    }
}

impl ValuePair {
    pub fn new(addr: u64, value_type: ValueType) -> Self {
        Self { addr, value_type }
    }
}

impl From<(u64, ValueType)> for ValuePair {
    fn from(tuple: (u64, ValueType)) -> Self {
        Self::new(tuple.0, tuple.1)
    }
}

lazy_static! {
    pub static ref PAGE_SIZE: usize = {
        nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE)
            .ok()
            .flatten()
            .filter(|&size| size > 0)
            .map(|size| size as usize)
            .unwrap_or(4096)
    };
    pub static ref PAGE_MASK: usize = !(*PAGE_SIZE - 1);
}

/// 很大，避免split
pub const BPLUS_TREE_ORDER: u16 = 256; // B+树阶数

pub trait SearchProgressCallback: Send + Sync {
    fn on_search_complete(&self, total_found: usize, total_regions: usize, elapsed_millis: u64);
}

pub struct SearchEngineManager {
    result_manager: Option<SearchResultManager>,
    chunk_size: usize,
    filter: SearchFilter,
}

impl SearchEngineManager {
    pub fn new() -> Self {
        Self {
            result_manager: None,
            chunk_size: 512 * 1024, // Default: 512KB
            filter: SearchFilter::new(),
        }
    }

    pub fn init(&mut self, memory_buffer_size: usize, cache_dir: String, chunk_size: usize) -> Result<()> {
        if self.result_manager.is_some() {
            log::warn!("SearchEngineManager already initialized, reinitializing...");
        }

        let cache_path = PathBuf::from(cache_dir);
        self.result_manager = Some(SearchResultManager::new(memory_buffer_size, cache_path));
        self.chunk_size = if chunk_size == 0 {
            512 * 1024 // Default to 512KB if 0 is passed
        } else {
            chunk_size
        };

        Ok(())
    }

    /// 判断是否初始化了，但是大部分情况下都是已经初始化了的
    pub fn is_initialized(&self) -> bool {
        self.result_manager.is_some()
    }

    /// 搜索内存，是属于新搜索的，就是会清除之前的搜索结果
    /// 使用 DriverManager 配置的 access_mode 进行内存读取
    /// 这个回调可能功能有点少了
    ///
    /// # Parameters
    /// - `use_deep_search`: 是否使用深度搜索模式（找到所有可能的组合）
    pub fn search_memory(
        &mut self,
        query: &SearchQuery,
        regions: &[(u64, u64)],
        use_deep_search: bool,
        callback: Option<Arc<dyn SearchProgressCallback>>,
    ) -> Result<usize> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.clear()?; // 清空搜索结果，有损耗吗？应该有吧（
        result_mgr.set_mode(SearchResultMode::Exact)?; // 这种搜索模式为精确搜索（包含范围值的搜索）

        let start_time = Instant::now();

        log::debug!(
            "Starting search: {} values, mode={:?}, range={}, regions={}, chunk_size={} KB, deep_search={}",
            query.values.len(),
            query.mode,
            query.range,
            regions.len(),
            self.chunk_size / 1024,
            use_deep_search
        );

        let chunk_size = self.chunk_size;
        let is_group_search = query.values.len() > 1;

        let all_results: Vec<BPlusTreeSet<ValuePair>> = regions
            .par_iter()
            .enumerate()
            .map(|(idx, (start, end))| {
                if log_enabled!(Level::Debug) {
                    log::debug!("Searching region {}: 0x{:X} - 0x{:X}", idx, start, end);
                }

                let result = if is_group_search {
                    if use_deep_search {
                        group_search::search_region_group_deep(query, *start, *end, chunk_size)
                    } else {
                        group_search::search_region_group(query, *start, *end, chunk_size)
                    }
                } else {
                    single_search::search_region(&query.values[0], *start, *end, chunk_size)
                };

                match result {
                    Ok(results) => results,
                    Err(e) => {
                        error!("Failed to search region {}: {:?}", idx, e);
                        BPlusTreeSet::new(BPLUS_TREE_ORDER)
                    },
                }
            })
            .collect();

        for region_results in all_results {
            if !region_results.is_empty() {
                // 搜索结果变例塞进Vec里面，这样后续的遍历什么的很舒服？
                // 问题上，er, 我们会出现删除结果集其中任意一个，或者任意多个的情况，使用Vec就会大量的重复拷贝，这样不好吧
                // todo 用B+树更好一些？
                let converted_results: Vec<SearchResultItem> =
                    region_results.into_iter().map(|pair| pair.into()).collect();
                result_mgr.add_results_batch(converted_results)?;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let final_count = result_mgr.total_count();

        if log_enabled!(Level::Debug) {
            log::info!("Search completed: {} results in {} ms", final_count, elapsed);
        }

        // 是否实现一个更丰富的回调接口？例如搜索进度之类的，问题上jvm的native调用会要求持有native lock
        // 这就导致性能下降非常严重，如果回调回去的话
        // todo: 我们应该弄个数字指针（DirectBuffer）?然后让java层自己去读？
        if let Some(ref cb) = callback {
            cb.on_search_complete(final_count, regions.len(), elapsed);
        }

        Ok(final_count)
    }

    /// 获取搜索结果 start 开始，size 个数
    pub fn get_results(&self, start: usize, size: usize) -> Result<Vec<SearchResultItem>> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.get_results(start, size)
    }

    /// 获取搜索结果总数
    pub fn get_total_count(&self) -> Result<usize> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        Ok(result_mgr.total_count())
    }

    /// 清除搜索结果
    pub fn clear_results(&mut self) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.clear()
    }

    /// 删除单个搜索结果
    pub fn remove_result(&mut self, index: usize) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.remove_result(index)
    }

    /// 删除多个搜索结果
    pub fn remove_results_batch(&mut self, indices: Vec<usize>) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.remove_results_batch(indices)
    }

    /// 设置搜索结果管理器的模式
    /// 这个过滤器只作用于get_results等方法，并不会影响实际的搜索结果存储
    pub fn set_filter(
        &mut self,
        enable_address_filter: bool,
        address_start: u64,
        address_end: u64,
        enable_type_filter: bool,
        type_ids: Vec<i32>,
    ) -> Result<()> {
        self.filter.enable_address_filter = enable_address_filter;
        self.filter.address_start = address_start;
        self.filter.address_end = address_end;

        self.filter.enable_type_filter = enable_type_filter;
        self.filter.type_ids = type_ids.iter().filter_map(|&id| ValueType::from_id(id)).collect();

        Ok(())
    }

    /// 清除搜索结果过滤器
    pub fn clear_filter(&mut self) -> Result<()> {
        self.filter.clear();
        Ok(())
    }

    /// 获取当前搜索结果过滤器
    pub fn get_filter(&self) -> &SearchFilter {
        &self.filter
    }

    /// 获取当前搜索结果模式
    pub fn get_current_mode(&self) -> Result<SearchResultMode> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        Ok(result_mgr.get_mode())
    }

    /// 细化搜索
    /// 使用 DriverManager 配置的 access_mode 进行内存读取
    pub fn refine_search(
        &mut self,
        query: &SearchQuery,
        callback: Option<Arc<dyn SearchProgressCallback>>,
    ) -> Result<usize> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        let current_results: Vec<_> = match result_mgr.get_mode() {
            SearchResultMode::Exact => result_mgr
                .get_all_exact_results()?
                .into_iter()
                .map(|result| ValuePair::new(result.address, result.typ))
                .collect(),
            SearchResultMode::Fuzzy => {
                todo!("FuzzySearchResultManager not implemented yet");
            },
        };

        if current_results.is_empty() {
            log::warn!("No results to refine");
            return Ok(0);
        }

        let start_time = Instant::now();
        let total_addresses = current_results.len();

        log::debug!(
            "Starting refine search: {} values, mode={:?}, existing results={}",
            query.values.len(),
            query.mode,
            total_addresses
        );

        // Clear current results and prepare for new ones
        result_mgr.clear()?;
        result_mgr.set_mode(SearchResultMode::Exact)?;

        // 判断是单值搜索还是组搜索
        if query.values.len() == 1 {
            let refined_results = single_search::refine_single_search(
                &current_results,
                &query.values[0],
            )?;

            if !refined_results.is_empty() {
                let converted_results: Vec<SearchResultItem> = refined_results
                    .into_iter()
                    .map(|pair| SearchResultItem::new_exact(pair.addr, pair.value_type))
                    .collect();
                result_mgr.add_results_batch(converted_results)?;
            }
        } else {
            let refined_results = group_search::refine_search_group_with_dfs(
                &current_results,
                query,
            )?;

            if !refined_results.is_empty() {
                let converted_results: Vec<SearchResultItem> = refined_results
                    .into_iter()
                    .map(|pair| SearchResultItem::new_exact(pair.addr, pair.value_type))
                    .collect();
                result_mgr.add_results_batch(converted_results)?;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let final_count = result_mgr.total_count();

        log::info!(
            "Refine search completed: {} -> {} results in {} ms",
            total_addresses,
            final_count,
            elapsed
        );

        if let Some(ref cb) = callback {
            // For refine search, we pass total_regions as 1 since we're not scanning regions
            cb.on_search_complete(final_count, 1, elapsed);
        }

        Ok(final_count)
    }

    // Test helper methods - delegate to sub-modules
    #[cfg(test)]
    pub fn search_in_buffer_with_status(
        buffer: &[u8],
        buffer_addr: u64,
        region_start: u64,
        region_end: u64,
        alignment: usize,
        search_value: &super::super::SearchValue,
        value_type: ValueType,
        page_status: &crate::wuwa::PageStatusBitmap,
        results: &mut BPlusTreeSet<ValuePair>,
        matches_checked: &mut usize,
    ) {
        single_search::search_in_buffer_with_status(
            buffer,
            buffer_addr,
            region_start,
            region_end,
            alignment,
            search_value,
            value_type,
            page_status,
            results,
            matches_checked,
        )
    }

    #[cfg(test)]
    pub fn try_match_group_at_address(buffer: &[u8], addr: u64, query: &SearchQuery) -> Option<Vec<usize>> {
        group_search::try_match_group_at_address(buffer, addr, query)
    }

    /// Deep group search - finds ALL possible combinations when there are duplicate values
    /// Uses DFS backtracking to exhaustively search for all valid combinations
    #[cfg(test)]
    pub fn search_in_buffer_group_deep(
        buffer: &[u8],
        buffer_addr: u64,
        region_start: u64,
        region_end: u64,
        min_element_size: usize,
        query: &SearchQuery,
        page_status: &crate::wuwa::PageStatusBitmap,
        results: &mut BPlusTreeSet<ValuePair>,
        matches_checked: &mut usize,
    ) {
        group_search::search_in_buffer_group_deep(
            buffer,
            buffer_addr,
            region_start,
            region_end,
            min_element_size,
            query,
            page_status,
            results,
            matches_checked,
        )
    }
}

lazy_static! {
    pub static ref SEARCH_ENGINE_MANAGER: RwLock<SearchEngineManager> = RwLock::new(SearchEngineManager::new());
}
