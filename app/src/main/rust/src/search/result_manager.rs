mod exact;
mod fuzzy;

use super::types::ValueType;
pub use crate::search::result_manager::exact::ExactSearchResultItem;
use crate::search::result_manager::exact::ExactSearchResultManager;
pub use crate::search::result_manager::fuzzy::{FuzzySearchResultItem, FuzzyValue};
use anyhow::{Result, anyhow};
use lazy_static::lazy_static;
use log::{Level, debug, info, log_enabled, trace, warn, error};
use memmap2::MmapMut;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::RwLock;
use crate::search::engine::ValuePair;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchResultMode {
    Exact,
    Fuzzy,
}

pub enum SearchResultItem {
    Exact(ExactSearchResultItem),
    Fuzzy(FuzzySearchResultItem),
}

impl From<(u64, ValueType)> for SearchResultItem {
    fn from(tuple: (u64, ValueType)) -> Self {
        SearchResultItem::Exact(ExactSearchResultItem::from(tuple))
    }
}

impl From<&ValuePair> for SearchResultItem {
    fn from(pair: &ValuePair) -> Self {
        SearchResultItem::Exact(ExactSearchResultItem::from((pair.addr, pair.value_type)))
    }
}

pub(crate) struct SearchResultManager {
    current_mode: SearchResultMode,
    exact: ExactSearchResultManager,
}

impl SearchResultManager {
    pub fn new(memory_buffer_size: usize, cache_dir: PathBuf) -> Self {
        Self {
            current_mode: SearchResultMode::Exact,
            exact: ExactSearchResultManager::new(memory_buffer_size, cache_dir),
        }
    }

    pub fn clear(&mut self) -> Result<()> {
        match self.current_mode {
            SearchResultMode::Exact => self.exact.clear(),
            SearchResultMode::Fuzzy => Err(anyhow!("FuzzySearchResultManager not implemented yet")),
        }
    }

    pub fn set_mode(&mut self, mode: SearchResultMode) -> Result<()> {
        if mode != self.current_mode {
            match self.current_mode {
                SearchResultMode::Exact => {
                    if let Err(e) = self.exact.clear_disk() {
                        error!("clear_disk failed: {:?}", e);
                    }
                },
                SearchResultMode::Fuzzy => {
                    return Err(anyhow!("FuzzySearchResultManager not implemented yet"));
                },
            }
        }
        self.current_mode = mode;
        Ok(())
    }

    pub fn add_result(&mut self, item: SearchResultItem) -> Result<()> {
        match (self.current_mode, item) {
            (SearchResultMode::Exact, SearchResultItem::Exact(exact_item)) => self.exact.add_result(exact_item),
            (SearchResultMode::Fuzzy, SearchResultItem::Fuzzy(_fuzzy_item)) => {
                Err(anyhow!("FuzzySearchResultManager not implemented yet"))
            },
            _ => Err(anyhow!("Mismatched SearchResultMode and SearchResultItem type")),
        }
    }

    pub fn add_results_batch(&mut self, results: Vec<SearchResultItem>) -> Result<()> {
        for result in results {
            self.add_result(result)?;
        }
        Ok(())
    }

    pub fn get_results(&self, start: usize, size: usize) -> Result<Vec<SearchResultItem>> {
        match self.current_mode {
            SearchResultMode::Exact => {
                let exact_results = self.exact.get_results(start, size)?;
                Ok(exact_results.into_iter().map(SearchResultItem::Exact).collect())
            },
            SearchResultMode::Fuzzy => Err(anyhow!("FuzzySearchResultManager not implemented yet")),
        }
    }

    pub fn total_count(&self) -> usize {
        match self.current_mode {
            SearchResultMode::Exact => self.exact.total_count(),
            SearchResultMode::Fuzzy => todo!(),
        }
    }

    pub fn remove_result(&mut self, index: usize) -> Result<()> {
        match self.current_mode {
            SearchResultMode::Exact => self.exact.remove_result(index),
            SearchResultMode::Fuzzy => Err(anyhow!("FuzzySearchResultManager not implemented yet")),
        }
    }

    pub fn remove_results_batch(&mut self, indices: Vec<usize>) -> Result<()> {
        match self.current_mode {
            SearchResultMode::Exact => self.exact.remove_results_batch(indices),
            SearchResultMode::Fuzzy => Err(anyhow!("FuzzySearchResultManager not implemented yet")),
        }
    }

    pub fn get_mode(&self) -> SearchResultMode {
        self.current_mode
    }

    pub fn get_all_exact_results(&self) -> Result<Vec<ExactSearchResultItem>> {
        match self.current_mode {
            SearchResultMode::Exact => self.exact.get_all_results(),
            SearchResultMode::Fuzzy => Err(anyhow!("Cannot get exact results in fuzzy mode")),
        }
    }
}
