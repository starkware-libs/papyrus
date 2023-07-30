#![warn(missing_docs)]
//! Functionality for executing Starknet transactions and contract entry points.

mod execution_utils;
mod state_reader;
#[cfg(test)]
#[path = "state_reader_test.rs"]
mod state_reader_test;
