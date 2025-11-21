//! Global state management for core components

use crate::core::driver_manager::DriverManager;
use lazy_static::lazy_static;
use std::sync::RwLock;

lazy_static! {
    pub static ref DRIVER_MANAGER: RwLock<DriverManager> = RwLock::new(DriverManager::new());
}