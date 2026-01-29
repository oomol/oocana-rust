use std::fmt;
use thiserror::Error;

/// Result alias
pub type Result<T, E = Error> = anyhow::Result<T, E>;

/// Error type for this library.
#[derive(Error, Debug)]
pub struct Error {
    pub msg: String,
    #[cfg(feature = "nightly")]
    backtrace: std::backtrace::Backtrace,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

macro_rules! impl_from_error {
    ($error_type:ty, $msg:expr) => {
        impl From<$error_type> for Error {
            fn from(err: $error_type) -> Self {
                Error {
                    msg: String::from($msg),
                    #[cfg(feature = "nightly")]
                    backtrace: std::backtrace::Backtrace::capture(),
                    source: Some(Box::new(err)),
                }
            }
        }
    };
}

// Implement the Display trait for our Error type.
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg)?;
        if let Some(source) = &self.source {
            write!(f, " Source: {}", *source)?;
        }
        Ok(())
    }
}

// Implement Default for Error
impl Default for Error {
    fn default() -> Self {
        Error {
            msg: "".to_string(),
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: None,
        }
    }
}

impl Error {
    /// Create a new Error instance.
    pub fn new(msg: &str) -> Self {
        Error {
            msg: msg.to_string(),
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: None,
        }
    }
    /// Create a new Error instance with a source error.
    pub fn with_source(msg: &str, source: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Error {
            msg: msg.to_string(),
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: Some(source),
        }
    }
}

// PoisonError is not Send + Sync, so we can't use the macro
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

impl_from_error!(std::io::Error, "IO Error");
impl_from_error!(log::SetLoggerError, "Logger Error");
impl_from_error!(serde_json::Error, "JSON Error");
impl_from_error!(serde_yaml::Error, "YAML Error");

impl From<std::string::String> for Error {
    fn from(value: String) -> Self {
        Error {
            msg: value,
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: None,
        }
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error {
            msg: value.to_string(),
            #[cfg(feature = "nightly")]
            backtrace: std::backtrace::Backtrace::capture(),
            source: None,
        }
    }
}
