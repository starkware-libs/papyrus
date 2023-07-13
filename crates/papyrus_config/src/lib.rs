//! Configuration utilities for a Starknet node.
//!
//! # Example
//!
//! ```
//! use std::collections::BTreeMap;
//! use std::fs::File;
//! use std::path::Path;
//!
//! use clap::Command;
//! use papyrus_config::dumping::{ser_param, SerializeConfig};
//! use papyrus_config::loading::load_and_process_config;
//! use papyrus_config::{ParamPath, SerializedParam};
//! use serde::{Deserialize, Serialize};
//! use tempfile::TempDir;
//!
//! #[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
//! struct ConfigExample {
//!     key: usize,
//! }
//!
//! impl SerializeConfig for ConfigExample {
//!     fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
//!         BTreeMap::from([ser_param("key", &self.key, "This is key description.")])
//!     }
//! }
//!
//! let dir = TempDir::new().unwrap();
//! let file_path = dir.path().join("config.json");
//! ConfigExample { key: 42 }.dump_to_file(&vec![], file_path.to_str().unwrap());
//! let file = File::open(file_path).unwrap();
//! let loaded_config = load_and_process_config::<ConfigExample>(
//!     file,
//!     Command::new("Program"),
//!     vec!["Program".to_owned(), "--key".to_owned(), "770".to_owned()],
//! )
//! .unwrap();
//! assert_eq!(loaded_config.key, 770);
//! ```

use clap::parser::MatchesError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type ParamPath = String;
pub type Description = String;

#[cfg(test)]
#[path = "config_test.rs"]
mod config_test;

mod command;
pub mod converters;
pub mod dumping;
pub mod loading;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SerializedParam {
    pub description: String,
    pub value: Value,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PointerParam {
    pub description: String,
    pub pointer_target: ParamPath,
}

#[derive(thiserror::Error, Debug)]
pub enum SubConfigError {
    #[error(transparent)]
    CommandInput(#[from] clap::error::Error),
    #[error(transparent)]
    MissingParam(#[from] serde_json::Error),
    #[error(transparent)]
    CommandMatches(#[from] MatchesError),
    #[error(transparent)]
    WriteDumpedConfig(#[from] std::io::Error),
    #[error("Insert a new param is not allowed.")]
    ParamNotFound { param_path: String },
    #[error("{target_param} is not found.")]
    PointerTargetNotFound { target_param: String },
    #[error("{pointing_param} is not found.")]
    PointerSourceNotFound { pointing_param: String },
    #[error("Changing {param_path} type from {before} to {after} is not allowed.")]
    ChangeParamType { param_path: String, before: Value, after: Value },
}
