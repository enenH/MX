//! Search engine implementation modules

pub mod filter;
pub mod single_search;
pub mod group_search;
pub mod manager;

// Re-export commonly used items
pub use manager::{SearchEngineManager, SEARCH_ENGINE_MANAGER, SearchProgressCallback, ValuePair, BPLUS_TREE_ORDER, PAGE_SIZE, PAGE_MASK};
pub use filter::SearchFilter;