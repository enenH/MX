use super::super::SearchResultItem;
use super::super::result_manager::{SearchResultManager, SearchResultMode};
use super::super::types::{SearchQuery, ValueType};
use super::filter::SearchFilter;
use super::group_search;
use super::shared_buffer::{SearchErrorCode, SearchStatus, SharedBuffer};
use super::single_search;
use crate::core::globals::TOKIO_RUNTIME;
use crate::search::result_manager::ExactSearchResultItem;
use anyhow::{Result, anyhow};
use bplustree::BPlusTreeSet;
use lazy_static::lazy_static;
use log::{Level, debug, error, info, log_enabled};
use rayon::prelude::*;
use std::cmp::Ordering as CmpOrdering;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Address and value type pair for storing search results.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValuePair {
    pub(crate) addr: u64,
    pub(crate) value_type: ValueType,
}

impl PartialOrd<Self> for ValuePair {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.addr.cmp(&other.addr))
    }
}

impl Ord for ValuePair {
    fn cmp(&self, other: &Self) -> CmpOrdering {
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

/// B+ tree order for search results. Large value to avoid splits.
pub const BPLUS_TREE_ORDER: u16 = 256;

/// Legacy callback interface for search progress. Kept for backward compatibility.
pub trait SearchProgressCallback: Send + Sync {
    fn on_search_complete(&self, total_found: usize, total_regions: usize, elapsed_millis: u64);
}

/// Search engine manager with async support.
pub struct SearchEngineManager {
    result_manager: Option<SearchResultManager>,
    chunk_size: usize,
    filter: SearchFilter,
    shared_buffer: SharedBuffer,
    cancel_token: Option<CancellationToken>,
    search_handle: Option<JoinHandle<()>>,
}

impl SearchEngineManager {
    pub fn new() -> Self {
        Self {
            result_manager: None,
            chunk_size: 512 * 1024,
            filter: SearchFilter::new(),
            shared_buffer: SharedBuffer::new(),
            cancel_token: None,
            search_handle: None,
        }
    }

    /// Sets the shared buffer for progress communication.
    pub fn set_shared_buffer(&mut self, ptr: *mut u8, len: usize) -> bool {
        self.shared_buffer.set(ptr, len)
    }

    /// Clears the shared buffer.
    pub fn clear_shared_buffer(&mut self) {
        self.shared_buffer.clear();
    }

    /// Checks if a search is currently running.
    pub fn is_searching(&self) -> bool {
        if let Some(ref handle) = self.search_handle {
            !handle.is_finished()
        } else {
            false
        }
    }

    /// Requests cancellation of the current search.
    pub fn request_cancel(&self) {
        if let Some(ref token) = self.cancel_token {
            token.cancel();
        }
    }

    pub fn init(&mut self, memory_buffer_size: usize, cache_dir: String, chunk_size: usize) -> Result<()> {
        if self.result_manager.is_some() {
            log::warn!("SearchEngineManager already initialized, reinitializing...");
        }

        let cache_path = PathBuf::from(cache_dir);
        self.result_manager = Some(SearchResultManager::new(memory_buffer_size, cache_path));
        self.chunk_size = if chunk_size == 0 { 512 * 1024 } else { chunk_size };

        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.result_manager.is_some()
    }

    /// Starts an async memory search. Returns immediately.
    /// Progress and status are communicated via the shared buffer.
    pub fn start_search_async(
        &mut self,
        query: SearchQuery,
        regions: Vec<(u64, u64)>,
        use_deep_search: bool,
    ) -> Result<()> {
        if !self.is_initialized() {
            self.shared_buffer.write_status(SearchStatus::Error);
            self.shared_buffer.write_error_code(SearchErrorCode::NotInitialized);
            return Err(anyhow!("SearchEngineManager not initialized"));
        }

        if self.is_searching() {
            self.shared_buffer.write_status(SearchStatus::Error);
            self.shared_buffer.write_error_code(SearchErrorCode::AlreadySearching);
            return Err(anyhow!("Search already in progress"));
        }

        // Prepare result manager.
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager's result_manager not initialized"))?;
        result_mgr.clear()?;
        result_mgr.set_mode(SearchResultMode::Exact)?;

        // Reset shared buffer and set searching status.
        self.shared_buffer.reset();
        self.shared_buffer.clear_cancel_flag();
        self.shared_buffer.write_status(SearchStatus::Searching);

        // Create new cancellation token.
        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let chunk_size = self.chunk_size;

        // Spawn async search task.
        let handle = TOKIO_RUNTIME.spawn(async move {
            Self::run_search_task(query, regions, use_deep_search, chunk_size, cancel_token).await;
        });

        self.search_handle = Some(handle);
        Ok(())
    }

    /// Internal async search task that runs in tokio runtime.
    async fn run_search_task(
        query: SearchQuery,
        regions: Vec<(u64, u64)>,
        use_deep_search: bool,
        chunk_size: usize,
        cancel_token: CancellationToken,
    ) {
        let start_time = Instant::now();
        let total_regions = regions.len();
        let is_group_search = query.values.len() > 1;

        if log_enabled!(Level::Debug) {
            debug!(
                "Starting async search: {} values, mode={:?}, range={}, regions={}, chunk_size={} KB, deep_search={}",
                query.values.len(),
                query.mode,
                query.range,
                regions.len(),
                chunk_size / 1024,
                use_deep_search
            );
        }

        // Shared state for progress tracking.
        let completed_regions = Arc::new(AtomicUsize::new(0));
        let total_found_count = Arc::new(AtomicI64::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));

        // Clone for the blocking task.
        let completed_regions_clone = Arc::clone(&completed_regions);
        let total_found_clone = Arc::clone(&total_found_count);
        let cancelled_clone = Arc::clone(&cancelled);
        let cancel_token_clone = cancel_token.clone();

        // Run the CPU-intensive search in a blocking task with rayon.
        let search_result = tokio::task::spawn_blocking(move || {
            let all_results: Vec<BPlusTreeSet<ValuePair>> = regions
                .par_iter()
                .enumerate()
                .filter_map(|(idx, (start, end))| {
                    // Check cancellation from both CancellationToken and shared buffer.
                    if cancel_token_clone.is_cancelled() || cancelled_clone.load(AtomicOrdering::Relaxed) {
                        cancelled_clone.store(true, AtomicOrdering::Relaxed);
                        return None;
                    }

                    // Check cancel flag from shared buffer.
                    if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                        if manager.shared_buffer.is_cancel_requested() {
                            cancelled_clone.store(true, AtomicOrdering::Relaxed);
                            return None;
                        }
                    }

                    // if log_enabled!(Level::Debug) {
                    //     debug!("Searching region {}: 0x{:X} - 0x{:X}", idx, start, end);
                    // }

                    // Create a cancel check closure for deep search.
                    // This closure also sets cancelled_clone to propagate cancellation to other parallel tasks.
                    let check_cancelled_for_region = || -> bool {
                        if cancel_token_clone.is_cancelled() || cancelled_clone.load(AtomicOrdering::Relaxed) {
                            cancelled_clone.store(true, AtomicOrdering::Relaxed);
                            return true;
                        }
                        if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                            if manager.shared_buffer.is_cancel_requested() {
                                cancelled_clone.store(true, AtomicOrdering::Relaxed);
                                return true;
                            }
                        }
                        false
                    };

                    let result = if is_group_search {
                        if use_deep_search {
                            // Use cancellable version for deep search.
                            group_search::search_region_group_deep_with_cancel(
                                &query,
                                *start,
                                *end,
                                chunk_size,
                                &check_cancelled_for_region,
                            )
                        } else {
                            group_search::search_region_group(&query, *start, *end, chunk_size)
                        }
                    } else {
                        single_search::search_region_single(&query.values[0], *start, *end, chunk_size)
                    };

                    let region_results = match result {
                        Ok(results) => results,
                        Err(e) => {
                            error!("Failed to search region {}: {:?}", idx, e);
                            BPlusTreeSet::new(BPLUS_TREE_ORDER)
                        },
                    };

                    // Update progress counters.
                    let completed = completed_regions_clone.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                    let found_in_region = region_results.len() as i64;
                    let total_found =
                        total_found_clone.fetch_add(found_in_region, AtomicOrdering::Relaxed) + found_in_region;

                    // Update shared buffer with progress information.
                    if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                        let progress = ((completed as f64 / total_regions as f64) * 100.0) as i32;
                        manager
                            .shared_buffer
                            .update_progress(progress, completed as i32, total_found);
                        manager.shared_buffer.tick_heartbeat();
                    }

                    if log_enabled!(Level::Debug) && completed % 100 == 0 {
                        let progress = ((completed as f64 / total_regions as f64) * 100.0) as i32;
                        debug!("Search progress: {}% ({}/{})", progress, completed, total_regions);
                    }

                    Some(region_results)
                })
                .collect();

            all_results
        })
        .await;

        // Check if cancelled.
        if cancel_token.is_cancelled() || cancelled.load(AtomicOrdering::Relaxed) {
            // Update shared buffer via the global manager.
            if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                manager.shared_buffer.write_status(SearchStatus::Cancelled);
            }
            info!("Search cancelled");
            return;
        }

        // Process results.
        // IMPORTANT: We must release the write lock BEFORE setting status to COMPLETED.
        // This ensures that when Kotlin sees COMPLETED status and calls getResults(),
        // the read lock can be acquired immediately.
        let (final_count, elapsed, success) = match search_result {
            Ok(all_results) => {
                match SEARCH_ENGINE_MANAGER.write() {
                    Ok(mut manager) => {
                        if let Some(ref mut result_mgr) = manager.result_manager {
                            for region_results in all_results {
                                if !region_results.is_empty() {
                                    let converted_results: Vec<SearchResultItem> =
                                        region_results.into_iter().map(|pair| pair.into()).collect();
                                    if let Err(e) = result_mgr.add_results_batch(converted_results) {
                                        error!("Failed to add results: {:?}", e);
                                    }
                                }
                            }

                            let elapsed = start_time.elapsed().as_millis() as u64;
                            let final_count = result_mgr.total_count();

                            info!("Search completed: {} results in {} ms", final_count, elapsed);

                            // Update progress info but NOT status yet (write lock still held).
                            manager.shared_buffer.write_found_count(final_count as i64);
                            manager.shared_buffer.write_progress(100);
                            manager.shared_buffer.write_regions_done(total_regions as i32);

                            (final_count as i64, elapsed, true)
                        } else {
                            error!("result_manager is None when processing search results");
                            (0, 0, false)
                        }
                    },
                    Err(e) => {
                        error!("Failed to acquire write lock for search results: {:?}", e);
                        (0, 0, false)
                    },
                }
                // Write lock is released here when `manager` goes out of scope.
            },
            Err(e) => {
                error!("Search task failed: {:?}", e);
                (0, 0, false)
            },
        };

        // Now set status AFTER the write lock is released.
        // This ensures Kotlin can immediately acquire read lock when it sees COMPLETED.
        if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
            if success {
                manager.shared_buffer.write_status(SearchStatus::Completed);
            } else {
                manager.shared_buffer.write_status(SearchStatus::Error);
                manager.shared_buffer.write_error_code(SearchErrorCode::InternalError);
            }
        }
    }

    /// Starts async refine search. Returns immediately.
    pub fn start_refine_async(&mut self, query: SearchQuery) -> Result<()> {
        if !self.is_initialized() {
            self.shared_buffer.write_status(SearchStatus::Error);
            self.shared_buffer.write_error_code(SearchErrorCode::NotInitialized);
            return Err(anyhow!("SearchEngineManager not initialized"));
        }

        if self.is_searching() {
            self.shared_buffer.write_status(SearchStatus::Error);
            self.shared_buffer.write_error_code(SearchErrorCode::AlreadySearching);
            return Err(anyhow!("Search already in progress"));
        }

        let result_mgr = self.result_manager.as_ref().unwrap();
        let current_results: Vec<ValuePair> = match result_mgr.get_mode() {
            SearchResultMode::Exact => result_mgr
                .get_all_exact_results()?
                .into_iter()
                .map(|result| ValuePair::new(result.address, result.typ))
                .collect(),
            SearchResultMode::Fuzzy => {
                return Err(anyhow!("Fuzzy mode refine search not implemented"));
            },
        };

        if current_results.is_empty() {
            log::warn!("No results to refine");
            self.shared_buffer.write_status(SearchStatus::Completed);
            self.shared_buffer.write_found_count(0);
            return Ok(());
        }

        // Reset shared buffer.
        self.shared_buffer.reset();
        self.shared_buffer.clear_cancel_flag();
        self.shared_buffer.write_status(SearchStatus::Searching);

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let handle = TOKIO_RUNTIME.spawn(async move {
            Self::run_refine_task(query, current_results, cancel_token).await;
        });

        self.search_handle = Some(handle);
        Ok(())
    }

    /// Internal async refine task.
    async fn run_refine_task(query: SearchQuery, current_results: Vec<ValuePair>, cancel_token: CancellationToken) {
        let start_time = Instant::now();
        let total_addresses = current_results.len();

        debug!(
            "Starting async refine search: {} values, mode={:?}, existing results={}",
            query.values.len(),
            query.mode,
            total_addresses
        );

        let processed_counter = Arc::new(AtomicUsize::new(0));
        let total_found_counter = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicBool::new(false));

        let processed_clone = Arc::clone(&processed_counter);
        let found_clone = Arc::clone(&total_found_counter);
        let cancelled_clone = Arc::clone(&cancelled);
        let cancel_token_clone = cancel_token.clone();

        let refine_result = tokio::task::spawn_blocking(move || {
            // Check cancellation from both CancellationToken and shared buffer.
            let check_cancelled = || -> bool {
                if cancel_token_clone.is_cancelled() || cancelled_clone.load(AtomicOrdering::Relaxed) {
                    return true;
                }
                if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                    if manager.shared_buffer.is_cancel_requested() {
                        cancelled_clone.store(true, AtomicOrdering::Relaxed);
                        return true;
                    }
                }
                false
            };

            if check_cancelled() {
                return Vec::new();
            }

            // Progress update callback for refine search.
            let update_progress = |processed: usize, found: usize| {
                if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                    let progress = ((processed as f64 / total_addresses as f64) * 100.0) as i32;
                    manager
                        .shared_buffer
                        .update_progress(progress, processed as i32, found as i64);
                    manager.shared_buffer.tick_heartbeat();
                }
            };

            let refined_results = if query.values.len() == 1 {
                single_search::refine_single_search_with_cancel(
                    &current_results,
                    &query.values[0],
                    Some(&processed_clone),
                    Some(&found_clone),
                    &check_cancelled,
                    &update_progress,
                )
                .unwrap_or_else(|e| {
                    error!("Refine search failed: {:?}", e);
                    Vec::new()
                })
            } else {
                match group_search::refine_search_group_with_dfs_and_cancel(
                    &current_results,
                    &query,
                    Some(&processed_clone),
                    Some(&found_clone),
                    &check_cancelled,
                    &update_progress,
                ) {
                    Ok(results) => results.into_iter().cloned().collect(),
                    Err(e) => {
                        error!("Group refine search failed: {:?}", e);
                        Vec::new()
                    },
                }
            };

            refined_results
        })
        .await;

        if cancel_token.is_cancelled() || cancelled.load(AtomicOrdering::Relaxed) {
            if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
                manager.shared_buffer.write_status(SearchStatus::Cancelled);
            }
            info!("Refine search cancelled");
            return;
        }

        // IMPORTANT: Release write lock BEFORE setting status to COMPLETED.
        let success = match refine_result {
            Ok(refined_results) => {
                match SEARCH_ENGINE_MANAGER.write() {
                    Ok(mut manager) => {
                        if let Some(ref mut result_mgr) = manager.result_manager {
                            // Clear and update results.
                            let _ = result_mgr.clear();
                            let _ = result_mgr.set_mode(SearchResultMode::Exact);

                            if !refined_results.is_empty() {
                                let converted_results: Vec<SearchResultItem> = refined_results
                                    .into_iter()
                                    .map(|pair| SearchResultItem::new_exact(pair.addr, pair.value_type))
                                    .collect();
                                let _ = result_mgr.add_results_batch(converted_results);
                            }

                            let elapsed = start_time.elapsed().as_millis() as u64;
                            let final_count = result_mgr.total_count();

                            info!(
                                "Refine search completed: {} -> {} results in {} ms",
                                total_addresses, final_count, elapsed
                            );

                            // Update progress info but NOT status yet.
                            manager.shared_buffer.write_found_count(final_count as i64);
                            manager.shared_buffer.write_progress(100);

                            true
                        } else {
                            error!("result_manager is None when processing refine results");
                            false
                        }
                    },
                    Err(e) => {
                        error!("Failed to acquire write lock for refine results: {:?}", e);
                        false
                    },
                }
                // Write lock released here.
            },
            Err(e) => {
                error!("Refine task failed: {:?}", e);
                false
            },
        };

        // Set status AFTER write lock is released.
        if let Ok(manager) = SEARCH_ENGINE_MANAGER.read() {
            if success {
                manager.shared_buffer.write_status(SearchStatus::Completed);
            } else {
                manager.shared_buffer.write_status(SearchStatus::Error);
                manager.shared_buffer.write_error_code(SearchErrorCode::InternalError);
            }
        }
    }

    /// Legacy synchronous search method. Kept for backward compatibility.
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

        result_mgr.clear()?;
        result_mgr.set_mode(SearchResultMode::Exact)?;

        let start_time = Instant::now();

        debug!(
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
        let total_regions = regions.len();

        let completed_regions = Arc::new(AtomicUsize::new(0));
        let total_found_count = Arc::new(AtomicI64::new(0));

        let all_results: Vec<BPlusTreeSet<ValuePair>> = regions
            .par_iter()
            .enumerate()
            .map(|(idx, (start, end))| {
                // if log_enabled!(Level::Debug) {
                //     debug!("Searching region {}: 0x{:X} - 0x{:X}", idx, start, end);
                // }

                let result = if is_group_search {
                    if use_deep_search {
                        group_search::search_region_group_deep(query, *start, *end, chunk_size)
                    } else {
                        group_search::search_region_group(query, *start, *end, chunk_size)
                    }
                } else {
                    single_search::search_region_single(&query.values[0], *start, *end, chunk_size)
                };

                let region_results = match result {
                    Ok(results) => results,
                    Err(e) => {
                        error!("Failed to search region {}: {:?}", idx, e);
                        BPlusTreeSet::new(BPLUS_TREE_ORDER)
                    },
                };

                let completed = completed_regions.fetch_add(1, AtomicOrdering::Relaxed) + 1;
                let found_in_region = region_results.len() as i64;
                total_found_count.fetch_add(found_in_region, AtomicOrdering::Relaxed);

                // Update shared buffer progress if set.
                if self.shared_buffer.is_set() {
                    let progress = ((completed as f64 / total_regions as f64) * 100.0) as i32;
                    let total_found = total_found_count.load(AtomicOrdering::Relaxed);
                    self.shared_buffer
                        .update_progress(progress, completed as i32, total_found);
                }

                region_results
            })
            .collect();

        for region_results in all_results {
            if !region_results.is_empty() {
                let converted_results: Vec<SearchResultItem> =
                    region_results.into_iter().map(|pair| pair.into()).collect();
                result_mgr.add_results_batch(converted_results)?;
            }
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let final_count = result_mgr.total_count();

        if log_enabled!(Level::Debug) {
            info!("Search completed: {} results in {} ms", final_count, elapsed);
        }

        if let Some(ref cb) = callback {
            cb.on_search_complete(final_count, regions.len(), elapsed);
        }

        Ok(final_count)
    }

    pub fn get_results(&self, start: usize, size: usize) -> Result<Vec<SearchResultItem>> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.get_results(start, size)
    }

    pub fn get_total_count(&self) -> Result<usize> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        Ok(result_mgr.total_count())
    }

    pub fn clear_results(&mut self) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.clear()
    }

    pub fn remove_result(&mut self, index: usize) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.remove_result(index)
    }

    pub fn remove_results_batch(&mut self, indices: Vec<usize>) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.remove_results_batch(indices)
    }

    pub fn keep_only_results(&mut self, keep_indices: Vec<usize>) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.keep_only_results(keep_indices)
    }

    pub fn set_result_mode(&mut self, mode: SearchResultMode) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.set_mode(mode)
    }

    pub fn add_results_batch(&mut self, results: Vec<SearchResultItem>) -> Result<()> {
        let result_mgr = self
            .result_manager
            .as_mut()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        result_mgr.add_results_batch(results)
    }

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

    pub fn clear_filter(&mut self) -> Result<()> {
        self.filter.clear();
        Ok(())
    }

    pub fn get_filter(&self) -> &SearchFilter {
        &self.filter
    }

    pub fn get_current_mode(&self) -> Result<SearchResultMode> {
        let result_mgr = self
            .result_manager
            .as_ref()
            .ok_or_else(|| anyhow!("SearchEngineManager not initialized"))?;

        Ok(result_mgr.get_mode())
    }

    /// Legacy synchronous refine search method.
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
                return Err(anyhow!("FuzzySearchResultManager not implemented yet"));
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

        let processed_counter = Arc::new(AtomicUsize::new(0));
        let total_found_counter = Arc::new(AtomicUsize::new(0));

        result_mgr.clear()?;
        result_mgr.set_mode(SearchResultMode::Exact)?;

        let refined_results = if query.values.len() == 1 {
            single_search::refine_single_search(
                &current_results,
                &query.values[0],
                Some(&processed_counter),
                Some(&total_found_counter),
            )?
        } else {
            let results = group_search::refine_search_group_with_dfs(
                &current_results,
                query,
                Some(&processed_counter),
                Some(&total_found_counter),
            )?;

            results.into_iter().cloned().collect()
        };

        total_found_counter.store(refined_results.len(), AtomicOrdering::Relaxed);

        if !refined_results.is_empty() {
            let converted_results: Vec<SearchResultItem> = refined_results
                .into_iter()
                .map(|pair| SearchResultItem::new_exact(pair.addr, pair.value_type))
                .collect();
            result_mgr.add_results_batch(converted_results)?;
        }

        let elapsed = start_time.elapsed().as_millis() as u64;
        let final_count = result_mgr.total_count();

        info!(
            "Refine search completed: {} -> {} results in {} ms",
            total_addresses, final_count, elapsed
        );

        if let Some(ref cb) = callback {
            cb.on_search_complete(final_count, 1, elapsed);
        }

        Ok(final_count)
    }

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
