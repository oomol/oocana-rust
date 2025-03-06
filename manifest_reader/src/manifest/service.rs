use serde::{Deserialize, Serialize};
use tracing::warn;

use super::ServiceBlock;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(from = "TmpServiceExecutorOptions")]
pub struct ServiceExecutorOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default)]
    pub start_at: StartAt,
    #[serde(default)]
    pub stop_at: StopAt,
    pub keep_alive: Option<u64>,
}

impl ServiceExecutorOptions {
    pub fn is_global(&self) -> bool {
        match self.stop_at {
            StopAt::Never => true,
            StopAt::App => true,
            _ => false,
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Service {
    pub executor: Option<ServiceExecutorOptions>,
    pub blocks: Option<Vec<ServiceBlock>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StartAt {
    #[serde(rename = "block_start")]
    Block,
    #[serde(rename = "session_start")]
    Session,
    #[serde(rename = "app_start")]
    App,
}

impl Default for StartAt {
    fn default() -> Self {
        StartAt::Block
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum StopAt {
    #[serde(rename = "block_end")]
    Block,
    #[serde(rename = "session_end")]
    Session,
    #[serde(rename = "app_end")]
    App,
    Never,
}

impl Default for StopAt {
    fn default() -> Self {
        StopAt::Session
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct TmpServiceExecutorOptions {
    pub name: Option<String>,
    pub entry: Option<String>,
    pub function: Option<String>,
    pub start_at: Option<String>,
    pub stop_at: Option<String>,
    pub keep_alive: Option<u64>,
}

impl From<TmpServiceExecutorOptions> for ServiceExecutorOptions {
    fn from(tmp: TmpServiceExecutorOptions) -> Self {
        Self {
            name: tmp.name,
            entry: tmp.entry,
            function: tmp.function,
            start_at: match tmp.start_at.as_deref() {
                Some("block_start") => StartAt::Block,
                Some("session_start") => StartAt::Session,
                Some("app_start") => StartAt::App,
                None => StartAt::Block,
                _ => {
                    warn!(
                        "Invalid start_at value: {:?}, use default value block_start",
                        tmp.start_at
                    );
                    StartAt::Block
                }
            },
            stop_at: match tmp.stop_at.as_deref() {
                Some("block_end") => StopAt::Block,
                Some("session_end") => StopAt::Session,
                Some("app_end") => StopAt::App,
                Some("never") => StopAt::Never,
                None => StopAt::Session,
                _ => {
                    warn!(
                        "Invalid stop_at value: {:?}, use default value: session_end",
                        tmp.stop_at
                    );
                    StopAt::Session
                }
            },
            keep_alive: tmp.keep_alive,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_executor_options() {
        use super::*;
        let json = r#"{"name":"test","entry":"main","function":"func","start_at":"block_start","stop_at":"session_end","keep_alive":100}"#;
        let executor: ServiceExecutorOptions = serde_json::from_str(json).unwrap();
        assert_eq!(executor.name, Some("test".to_string()));
        assert_eq!(executor.entry, Some("main".to_string()));
        assert_eq!(executor.function, Some("func".to_string()));
        assert_eq!(executor.start_at, StartAt::Block);
        assert_eq!(executor.stop_at, StopAt::Session);
        assert_eq!(executor.keep_alive, Some(100));
    }

    #[test]
    fn test_executor_options_default() {
        use super::*;
        let json = r#"{"keep_alive":100}"#;
        let executor: ServiceExecutorOptions = serde_json::from_str(json).unwrap();
        assert_eq!(executor.name, None);
        assert_eq!(executor.entry, None);
        assert_eq!(executor.function, None);
        assert_eq!(executor.start_at, StartAt::Block);
        assert_eq!(executor.stop_at, StopAt::Session);
        assert_eq!(executor.keep_alive, Some(100));
    }

    #[test]
    fn test_executor_options_wrong_alive() {
        use super::*;
        let json = r#"{"name":"test","entry":"main","function":"func","start_at":"block_start","stop_at":"session_end","keep_alive":"100"}"#;
        let r: Result<ServiceExecutorOptions, _> = serde_json::from_str(json);
        println!("{:?}", r);
        assert!(r.is_err());
    }

    #[test]
    fn test_executor_options_wrong_stop_at() {
        use super::*;
        let json = r#"{"name":"test","entry":"main","function":"func","start_at":"block_start","stop_at":"wrong","keep_alive":100}"#;
        let r: Result<ServiceExecutorOptions, _> = serde_json::from_str(json);

        let option = r.unwrap();
        assert_eq!(option.stop_at, StopAt::Session);
        assert_eq!(option.start_at, StartAt::Block);
    }
}
