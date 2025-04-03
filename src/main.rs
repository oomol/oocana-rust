#[cfg(not(debug_assertions))]
use human_panic::setup_panic;

#[cfg(debug_assertions)]
extern crate better_panic;

#[cfg(feature = "backtrace")]
use std::env;

use utils::error::Result;

/// The main entry point of the application.
fn main() -> Result<()> {
    unsafe {
        #[cfg(feature = "backtrace")]
        env::set_var("RUST_BACKTRACE", "1")
    };

    // Human Panic. Only enabled when *not* debugging.
    #[cfg(not(debug_assertions))]
    {
        setup_panic!();
    }

    // Better Panic. Only enabled *when* debugging.
    #[cfg(debug_assertions)]
    {
        better_panic::Settings::debug()
            .most_recent_first(false)
            .lineno_suffix(true)
            .verbosity(better_panic::Verbosity::Full)
            .install();
    }

    // Match Commands
    cli::cli_match()?;

    Ok(())
}
