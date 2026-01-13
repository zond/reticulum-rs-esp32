//! Routing functionality for Reticulum transport nodes.
//!
//! This module provides:
//! - [`PathTable`]: Routing table for destination paths

mod path_table;

pub use path_table::{
    InterfaceType, PathEntry, PathTable, PathTableConfig, PathTableError, RoutingMetrics,
};
