#![cfg_attr(feature = "nightly", feature(backtrace))]

use hex;
use sha2::{Digest, Sha256};
pub mod cache;
pub mod error;
pub mod logger;
pub mod output;
pub mod path;
pub mod settings;

pub fn log_error(err: impl std::fmt::Debug) {
    tracing::error!("{:?}", err)
}

pub fn log_warn(warn: impl std::fmt::Debug) {
    tracing::warn!("{:?}", warn)
}

/// 根据 一串 input 生成一串 hash 值，避免 suffix 的冲突
pub fn calculate_short_hash(input: &str, length: usize) -> String {
    // Create a Sha256 object
    let mut hasher = Sha256::new();

    // Write input data
    hasher.update(input);

    // Read hash digest and consume hasher
    let result = hasher.finalize();

    // Convert the hash to a hexadecimal string
    let hex_result = hex::encode(result);

    // Return the first `length` characters of the hash
    hex_result[..length].to_string()
}
