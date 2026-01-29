use clap::Subcommand;
use utils::error::Result;

#[derive(Debug, Subcommand)]
pub enum CacheAction {
    #[command(about = "clear cache")]
    Clear {},
}

pub fn cache_action(action: &CacheAction) -> Result<()> {
    match action {
        CacheAction::Clear {} => {
            let cache_file = utils::cache::cache_meta_file_path();
            if cache_file.exists() {
                std::fs::remove_file(cache_file)?;
            }

            let cache_dir = utils::cache::cache_dir();
            if cache_dir.exists() {
                std::fs::remove_dir_all(cache_dir)?;
            }

            Ok(())
        }
    }
}
