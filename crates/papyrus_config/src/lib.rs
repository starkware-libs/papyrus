//! Configuration utilities for a Starknet node.
//!
//! This crate divides into two parts: defining a default config and loading a custom config based
//! on the default values.
//!
//! The default configuration is serialized into `default_config.json` file, you should never edit
//! it manually.
//! Before the node is running, the configuration may be updated by:
//! a) a custom config file, given in the command line flag `config_file`, and
//! b) environment variables and command line flags. The args are flattened and documented in
//! `default_config.json` file. The environment variables should be set in uppercase.

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
    #[error("Insert a new param is not allowed.")]
    ParamNotFound { param_path: String },
    #[error("{target_param} is not found.")]
    PointerTargetNotFound { target_param: String },
    #[error("{pointing_param} is not found.")]
    PointerSourceNotFound { pointing_param: String },
    #[error("Changing {param_path} type from {before} to {after} is not allowed.")]
    ChangeParamType { param_path: String, before: Value, after: Value },
}
