use job::SessionId;

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};
use std::path::Path;

use crate::delay_abort::DelayAbortTx;
use crate::remote_task_config::RemoteTaskConfig;

pub struct Shared {
    pub session_id: SessionId,
    pub address: String,
    pub connector_base_url: Option<String>,
    pub connector_auth_token: Option<String>,
    pub scheduler_tx: SchedulerTx,
    pub delay_abort_tx: DelayAbortTx,
    pub reporter: ReporterTx,
    pub use_cache: bool,
    pub remote_task_config: Option<RemoteTaskConfig>,
}

pub(crate) fn should_enable_package_layer(package_path: &Path, feature_enabled: bool) -> bool {
    feature_enabled
        && !manifest_reader::reader::should_skip_package_layer_handling_for_path(package_path)
}

pub(crate) fn package_scope_enable_layer(package_path: &Path) -> bool {
    should_enable_package_layer(package_path, layer::feature_enabled())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn connector_packages_disable_package_layer_even_when_feature_is_enabled() {
        let package = workspace_root().join("tests/fixtures/@connector/demo");

        assert!(!should_enable_package_layer(&package, true));
    }

    #[test]
    fn hide_source_packages_disable_package_layer_even_when_feature_is_enabled() {
        let package = workspace_root().join("examples/remote_task/hide-example");

        assert!(!should_enable_package_layer(&package, true));
    }

    #[test]
    fn regular_packages_still_enable_package_layer_when_feature_is_enabled() {
        let package = workspace_root().join("examples/base/pkg_a");

        assert!(should_enable_package_layer(&package, true));
    }
}
