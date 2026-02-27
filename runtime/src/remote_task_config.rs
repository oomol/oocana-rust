#[derive(Debug, Clone)]
pub struct RemoteTaskConfig {
    pub base_url: String,
    pub auth_token: Option<String>,
    /// Global timeout for remote task execution, in seconds.
    pub timeout_secs: Option<u64>,
}

impl RemoteTaskConfig {
    /// CLI url takes priority over env var. Auth token reuses OOMOL_TOKEN.
    /// Returns None if no URL is configured.
    pub fn from_env_and_args(
        cli_url: Option<&str>,
        cli_timeout: Option<u64>,
    ) -> Option<Self> {
        let base_url = cli_url
            .map(|s| s.to_owned())
            .or_else(|| std::env::var("OOCANA_TASK_API_URL").ok())?;

        let auth_token = std::env::var("OOMOL_TOKEN").ok();

        let timeout_secs = cli_timeout.or_else(|| {
            std::env::var("OOCANA_TASK_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
        });

        Some(Self {
            base_url,
            auth_token,
            timeout_secs,
        })
    }
}
