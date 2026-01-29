use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
};
use tracing::info;

use crate::{config, env};

use super::error::Result;

use std::sync::{Mutex, OnceLock};
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking;
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter, Layer, Registry};

static LOGGER_DIR: OnceLock<Mutex<PathBuf>> = OnceLock::new();

fn get_logger_dir() -> &'static Mutex<PathBuf> {
    LOGGER_DIR.get_or_init(|| {
        Mutex::new(config::oocana_dir().unwrap_or_else(std::env::temp_dir))
    })
}

pub fn logger_dir() -> PathBuf {
    get_logger_dir().lock().unwrap().clone()
}

pub const STDOUT_TARGET: &str = "stdout";
pub const STDERR_TARGET: &str = "stderr";

pub struct LogParams<'a, P: AsRef<Path>> {
    pub sub_dir: Option<P>,
    pub log_name: &'a str,
    pub output_to_console: bool,
    /// 捕获 target 为 STDOUT_TARGET 或者 STDERR_TARGET 的日志。
    pub capture_stdout_stderr_target: bool,
}

#[allow(unused_mut)]
pub fn setup_logging<P: AsRef<Path>>(params: LogParams<P>) -> Result<non_blocking::WorkerGuard> {
    let LogParams {
        sub_dir,
        log_name,
        mut output_to_console,
        capture_stdout_stderr_target,
    } = params;

    let mut logger_dir = config::oocana_dir().unwrap_or_else(std::env::temp_dir);

    if let Some(sub_dir) = sub_dir {
        logger_dir.push(sub_dir);
    }

    *get_logger_dir().lock().unwrap() = logger_dir.clone();

    // Set OVMLAYER environment variable for ovmlayer logging
    std::env::set_var(
        env::OVMLAYER_LOG_ENV_KEY,
        logger_dir
            .join("ovmlayer.log")
            .to_string_lossy()
            .to_string(),
    );

    let file_path = logger_dir.join(format!("{}.log", log_name));

    let f = create_file_with_dirs(&file_path)?;

    let (non_blocking, guard) = non_blocking(f);
    let file_layer = fmt::Layer::default()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_filter(LevelFilter::TRACE);

    let subscriber = Registry::default().with(file_layer);

    // 主动要求输出时，不使用 ansi color
    #[allow(unused_assignments, unused_mut)]
    let mut show_ansi_color = !output_to_console;

    #[cfg(debug_assertions)]
    {
        output_to_console = true;
        show_ansi_color = true;
    }

    if output_to_console {
        // 使用 target 提取出原始内容更方便。如果用 span，需要使用自定义 format 来提取原始信息（不带 span 的）
        if capture_stdout_stderr_target {
            let stdout_layer = fmt::Layer::default()
                .with_writer(std::io::stdout)
                .with_target(false)
                .with_ansi(show_ansi_color)
                .with_level(false)
                .without_time()
                .with_filter(
                    EnvFilter::from_default_env()
                        .add_directive(format!("{STDOUT_TARGET}=trace").parse().unwrap()),
                );

            let stderr_layer = fmt::Layer::default()
                .with_writer(std::io::stderr)
                .with_target(false)
                .with_ansi(show_ansi_color)
                .with_level(false)
                .without_time()
                .with_filter(
                    EnvFilter::from_default_env()
                        .add_directive(format!("{STDERR_TARGET}=trace").parse().unwrap()),
                );

            let subscriber = subscriber.with(stdout_layer).with(stderr_layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("Unable to set global subscriber");
        } else {
            let stdout_layer = fmt::Layer::default()
                .with_writer(std::io::stdout)
                .with_target(false)
                .with_ansi(show_ansi_color);

            #[cfg(debug_assertions)]
            let stdout_layer = stdout_layer.with_file(true).with_line_number(true);
            let subscriber = subscriber.with(stdout_layer);
            tracing::subscriber::set_global_default(subscriber)
                .expect("Unable to set global subscriber");
        }
    } else {
        tracing::subscriber::set_global_default(subscriber)
            .expect("Unable to set global subscriber");
    }

    info!("Logging to file: {:?}", file_path);

    Ok(guard)
}

fn create_file_with_dirs<P: AsRef<Path>>(file_path: P) -> io::Result<File> {
    if let Some(parent) = file_path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    // File::create(file_path)
    File::options().create(true).append(true).open(file_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger() {
        // Initialize config before using logger
        let _ = config::load_config(None::<String>);

        let g = setup_logging(LogParams {
            sub_dir: Some("test"),
            log_name: "test",
            output_to_console: true,
            capture_stdout_stderr_target: true,
        })
        .unwrap();

        tracing::info!(target: "stdout", "stdout");
        tracing::info!(target: "stderr", "stderr");

        drop(g);
    }
}
