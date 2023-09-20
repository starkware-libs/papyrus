// config compiler to support coverage_attribute feature when running coverage in nightly mode
// within this crate
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

#[allow(unused_imports)]
pub mod config;
#[cfg(test)]
mod precision_test;
pub mod version;
