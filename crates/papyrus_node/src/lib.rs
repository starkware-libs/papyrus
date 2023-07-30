// config compiler to support no_coverage feature when running coverage in nightly mode within this
// crate
#![cfg_attr(coverage_nightly, feature(no_coverage))]

#[allow(unused_imports)]
pub mod config;
pub mod version;
