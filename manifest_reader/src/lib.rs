pub mod manifest; // 这个 mod 尽量只给 meta 模块使用
pub mod path_finder;
pub mod reader;

pub use manifest::PackageMeta as Package;
pub use serde_json::Value as JsonValue;
