use std::collections::HashMap;

pub fn resolve_auth_token(env_file_vars: &HashMap<String, String>) -> Option<String> {
    std::env::var("OOMOL_TOKEN")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            env_file_vars
                .get("OOMOL_TOKEN")
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
        })
}

#[derive(Debug, Clone)]
pub struct RemoteTaskConfig {
    pub base_url: String,
    pub auth_token: Option<String>,
    /// Global timeout for remote task execution, in seconds.
    pub timeout_secs: Option<u64>,
}

impl RemoteTaskConfig {
    /// CLI url takes priority over env var, then env_file as fallback.
    /// Auth token reuses OOMOL_TOKEN.
    /// Returns None if no URL is configured.
    pub fn from_env_and_args(
        cli_url: Option<&str>,
        cli_timeout: Option<u64>,
        env_file_vars: &HashMap<String, String>,
    ) -> Option<Self> {
        let base_url = cli_url
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_owned())
            .or_else(|| {
                std::env::var("OOCANA_REMOTE_BLOCK_URL")
                    .ok()
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                env_file_vars
                    .get("OOCANA_REMOTE_BLOCK_URL")
                    .map(|s| s.trim().to_owned())
                    .filter(|s| !s.is_empty())
            })?;

        let auth_token = resolve_auth_token(env_file_vars);

        let timeout_secs = cli_timeout.or_else(|| {
            std::env::var("OOCANA_REMOTE_BLOCK_TIMEOUT")
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    env_file_vars
                        .get("OOCANA_REMOTE_BLOCK_TIMEOUT")
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                })
                .and_then(|s| match s.parse() {
                    Ok(v) => Some(v),
                    Err(_) => {
                        tracing::warn!(
                            "Invalid OOCANA_REMOTE_BLOCK_TIMEOUT value '{}', ignoring",
                            s
                        );
                        None
                    }
                })
        });

        Some(Self {
            base_url,
            auth_token,
            timeout_secs,
        })
    }
}
