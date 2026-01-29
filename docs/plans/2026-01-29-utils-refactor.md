# Utils Module Refactoring Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 重构 utils 模块，消除重复代码，简化数据结构，移除虚假的 Option 返回类型。

**Architecture:**
1. 将配置系统从 `Mutex<AppConfig>` 改为 `OnceLock<AppConfig>`，配置只加载一次后不可变
2. 消除 `TmpXxxConfig` 中间层，使用 serde 自定义反序列化
3. 统一错误处理，用宏消除重复的 `From` 实现

**Tech Stack:** Rust, serde, thiserror, OnceLock

---

## Task 1: 简化 cache.rs 的重复代码

**Files:**
- Modify: `utils/src/cache.rs:10-28`
- Test: `utils/tests/test.rs` (如果有相关测试)

**Step 1: 阅读现有测试**

检查是否有 cache 模块的测试。

**Step 2: 重构 cache.rs**

将两个重复的函数合并为一个辅助函数：

```rust
use crate::config;
use std::path::PathBuf;

pub const CACHE_META_FILE: &str = "cache_meta.json";

fn with_oocana_subpath(subpath: &str) -> Option<PathBuf> {
    config::oocana_dir().map(|mut p| {
        p.push(subpath);
        p
    })
}

pub fn cache_meta_file_path() -> Option<PathBuf> {
    with_oocana_subpath(CACHE_META_FILE)
}

pub fn cache_dir() -> Option<PathBuf> {
    with_oocana_subpath("cache")
}
```

**Step 3: 编译验证**

Run: `cargo build -p utils`
Expected: PASS

**Step 4: 运行测试**

Run: `cargo test -p utils`
Expected: PASS

**Step 5: Commit**

```bash
git add utils/src/cache.rs
git commit -m "refactor(utils): simplify cache.rs with helper function"
```

---

## Task 2: 消除 error.rs 中的重复 From 实现

**Files:**
- Modify: `utils/src/error.rs`

**Step 1: 创建宏消除重复**

在 `error.rs` 顶部添加宏：

```rust
macro_rules! impl_from_error {
    ($error_type:ty, $msg:expr) => {
        impl From<$error_type> for Error {
            fn from(err: $error_type) -> Self {
                Error {
                    msg: format!("{}: {}", $msg, err),
                    #[cfg(feature = "nightly")]
                    backtrace: std::backtrace::Backtrace::capture(),
                    source: Some(Box::new(err)),
                }
            }
        }
    };
    // 无 source 版本
    ($error_type:ty, $msg:expr, no_source) => {
        impl From<$error_type> for Error {
            fn from(_err: $error_type) -> Self {
                Error {
                    msg: String::from($msg),
                    #[cfg(feature = "nightly")]
                    backtrace: std::backtrace::Backtrace::capture(),
                    source: None,
                }
            }
        }
    };
}
```

**Step 2: 使用宏替换手写的 From 实现**

删除 60-135 行的 6 个手写 `impl From`，替换为：

```rust
impl_from_error!(std::io::Error, "IO Error");
impl_from_error!(log::SetLoggerError, "Logger Error");
impl_from_error!(serde_json::Error, "JSON Error");
impl_from_error!(serde_yaml::Error, "YAML Error");
```

对于 `PoisonError` 使用 no_source 版本（因为它不是 Send + Sync）：

```rust
impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error {
            msg: String::from("Poison Error"),
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: None,
        }
    }
}
```

保留 `From<String>` 和 `From<&str>` 因为它们逻辑不同。

**Step 3: 编译验证**

Run: `cargo build -p utils`
Expected: PASS

**Step 4: 运行测试**

Run: `cargo test -p utils`
Expected: PASS

**Step 5: Commit**

```bash
git add utils/src/error.rs
git commit -m "refactor(utils): use macro to reduce From impl duplication"
```

---

## Task 3: 将配置系统从 Mutex 改为 OnceLock

**Files:**
- Modify: `utils/src/config/app.rs`
- Modify: `utils/src/config/mod.rs`

**Step 1: 修改 app.rs 使用 OnceLock**

```rust
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use config::Config;
use dirs::home_dir;

use super::{global_config::GlobalConfig, run_config::RunConfig};
use crate::path::expand_home;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub global: GlobalConfig,
    pub run: RunConfig,
}

static GLOBAL_CONFIG: OnceLock<AppConfig> = OnceLock::new();

pub fn get_config() -> &'static AppConfig {
    GLOBAL_CONFIG.get().expect("Config not initialized. Call load_config first.")
}

pub fn load_config<P: AsRef<Path>>(file: Option<P>) -> Result<&'static AppConfig, String> {
    let p: PathBuf = match file {
        Some(p) => p.as_ref().to_path_buf(),
        None => {
            let mut default_path = home_dir().ok_or("Failed to get home dir")?;
            default_path.push("oocana");
            default_path.push("config");
            default_path
        }
    };

    let config_path = if p.is_absolute() {
        p
    } else {
        let expanded = PathBuf::from(expand_home(&p));
        if expanded.is_relative() {
            let mut current_dir = std::env::current_dir()
                .map_err(|e| format!("Failed to get current dir: {:?}", e))?;
            current_dir.push(expanded);
            current_dir
        } else {
            expanded
        }
    };

    let config = if !config_path.with_extension("json").exists()
        && !config_path.with_extension("toml").exists()
        && !config_path.with_extension("json5").exists()
    {
        tracing::info!(
            "No config file found at {:?}, using default config",
            config_path
        );
        AppConfig {
            global: GlobalConfig::default(),
            run: RunConfig::default(),
        }
    } else {
        Config::builder()
            .add_source(config::File::with_name(&config_path.to_string_lossy()))
            .build()
            .map_err(|e| format!("Failed to load config: {:?}", e))?
            .try_deserialize::<AppConfig>()
            .map_err(|e| format!("Failed to deserialize config: {:?}", e))?
    };

    GLOBAL_CONFIG.set(config).map_err(|_| "Config already initialized")?;
    Ok(get_config())
}
```

**Step 2: 修改 mod.rs 使用新的 get_config()**

```rust
mod app;
mod global_config;
mod run_config;
pub use app::*;

use std::path::PathBuf;

pub fn default_broker_port() -> u16 {
    47688
}

pub fn store_dir() -> PathBuf {
    PathBuf::from(&get_config().global.store_dir)
}

pub fn oocana_dir() -> PathBuf {
    PathBuf::from(&get_config().global.oocana_dir)
}

pub fn registry_store_file() -> PathBuf {
    use crate::path::expand_home;

    if let Ok(val) = std::env::var("OOMOL_REGISTRY_STORE_FILE") {
        if !val.is_empty() {
            return PathBuf::from(expand_home(&val));
        }
    }

    PathBuf::from(&get_config().global.registry_store_file)
}

pub fn search_paths() -> Option<Vec<String>> {
    get_config().global.search_paths.clone()
}

pub fn extra_search_path() -> Option<Vec<String>> {
    get_config()
        .run
        .extra
        .as_ref()
        .and_then(|e| e.search_paths.clone())
}

pub fn env_file() -> Option<String> {
    get_config().global.env_file.clone()
}

pub fn bind_path_file() -> Option<String> {
    get_config().global.bind_path_file.clone()
}
```

**Step 3: 删除 lazy_static 依赖（如果不再需要）**

检查 utils crate 中是否还有其他地方使用 lazy_static，如果没有，从 Cargo.toml 移除。

**Step 4: 编译验证**

Run: `cargo build -p utils`
Expected: PASS

**Step 5: 运行测试**

Run: `cargo test -p utils`
Expected: PASS

**Step 6: Commit**

```bash
git add utils/src/config/app.rs utils/src/config/mod.rs utils/Cargo.toml
git commit -m "refactor(utils): replace Mutex with OnceLock for config"
```

---

## Task 4: 更新依赖 config 模块的代码

**Files:**
- 需要搜索整个项目中调用 `store_dir()`、`oocana_dir()` 等返回 `Option<PathBuf>` 的地方

**Step 1: 搜索受影响的代码**

Run: `grep -r "store_dir()" --include="*.rs" /Users/yleaf/oomol/oocana-rust`
Run: `grep -r "oocana_dir()" --include="*.rs" /Users/yleaf/oomol/oocana-rust`
Run: `grep -r "registry_store_file()" --include="*.rs" /Users/yleaf/oomol/oocana-rust`

**Step 2: 更新调用点**

将所有 `.unwrap()` 或 `match Some(x)` 的模式改为直接使用返回值。

例如，将：
```rust
if let Some(dir) = config::oocana_dir() {
    // use dir
}
```

改为：
```rust
let dir = config::oocana_dir();
// use dir
```

**Step 3: 编译验证**

Run: `cargo build`
Expected: PASS

**Step 4: 运行测试**

Run: `cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add -A
git commit -m "refactor: update callers for new config API (non-optional return)"
```

---

## Task 5: 简化 logger.rs 中的 oocana_dir 调用

**Files:**
- Modify: `utils/src/logger.rs:45`

**Step 1: 更新 logger.rs**

将：
```rust
let mut logger_dir = config::oocana_dir().unwrap_or_else(std::env::temp_dir);
```

改为：
```rust
let mut logger_dir = config::oocana_dir();
```

因为 `oocana_dir()` 现在总是返回有效路径。

**Step 2: 编译验证**

Run: `cargo build -p utils`
Expected: PASS

**Step 3: Commit**

```bash
git add utils/src/logger.rs
git commit -m "refactor(utils): simplify logger.rs after config API change"
```

---

## Task 6: 更新 cache.rs 适配新的 config API

**Files:**
- Modify: `utils/src/cache.rs`

**Step 1: 修改 cache.rs**

由于 `oocana_dir()` 现在返回 `PathBuf` 而非 `Option<PathBuf>`，更新辅助函数：

```rust
use crate::config;
use std::path::PathBuf;

pub const CACHE_META_FILE: &str = "cache_meta.json";

fn with_oocana_subpath(subpath: &str) -> PathBuf {
    let mut p = config::oocana_dir();
    p.push(subpath);
    p
}

pub fn cache_meta_file_path() -> PathBuf {
    with_oocana_subpath(CACHE_META_FILE)
}

pub fn cache_dir() -> PathBuf {
    with_oocana_subpath("cache")
}
```

**Step 2: 更新调用 cache 模块的代码**

搜索并更新调用点。

**Step 3: 编译验证**

Run: `cargo build`
Expected: PASS

**Step 4: Commit**

```bash
git add utils/src/cache.rs
git commit -m "refactor(utils): cache.rs now returns PathBuf directly"
```

---

## Task 7: (可选) 消除 TmpConfig 中间层

**优先级：低** - 这个改动工作量较大，收益相对较小。如果前面的任务完成后有时间再做。

**思路：** 使用 serde 的 `#[serde(deserialize_with = "...")]` 在字段级别处理 `expand_home`，而不是整个结构体搞中间层。

```rust
fn deserialize_expand_home<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(expand_home(&s))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GlobalConfig {
    #[serde(default = "default_store_dir", deserialize_with = "deserialize_expand_home")]
    pub store_dir: String,
    // ...
}
```

---

## 执行顺序建议

1. **Task 1** (cache.rs) - 独立，风险低
2. **Task 2** (error.rs) - 独立，风险低
3. **Task 3** (OnceLock) - 核心改动
4. **Task 4** (更新调用点) - 依赖 Task 3
5. **Task 5** (logger.rs) - 依赖 Task 3
6. **Task 6** (cache.rs 适配) - 依赖 Task 3
7. **Task 7** (TmpConfig) - 可选，低优先级

---

## 风险评估

| Task | 风险 | 原因 |
|------|-----|------|
| Task 1-2 | 低 | 纯重构，不改变行为 |
| Task 3 | 中 | 改变配置加载模式，需确保 load_config 在使用前调用 |
| Task 4-6 | 中 | API 签名变更，需要更新所有调用点 |
| Task 7 | 低 | 可选优化，不影响功能 |

---

## 回滚策略

如果 Task 3-6 导致问题，可以暂时保留 `Option` 返回类型但内部使用 OnceLock：

```rust
pub fn oocana_dir() -> Option<PathBuf> {
    Some(PathBuf::from(&get_config().global.oocana_dir))
}
```

这样调用点不需要修改，但仍然消除了 Mutex 的性能问题。
