#![cfg_attr(feature = "nightly", feature(backtrace))]

pub mod app_config;
pub mod error;
pub mod logger;
pub mod output;
pub mod path;
pub mod types;

pub fn log_error(err: impl std::fmt::Debug) {
    log::error!("{:?}", err)
}

pub fn log_warn(warn: impl std::fmt::Debug) {
    log::warn!("{:?}", warn)
}
