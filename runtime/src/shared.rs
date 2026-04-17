use job::SessionId;

use mainframe::{reporter::ReporterTx, scheduler::SchedulerTx};

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

pub(crate) fn should_enable_package_layer(
    hide_source: bool,
    package_name: Option<&str>,
    feature_enabled: bool,
) -> bool {
    feature_enabled
        && !manifest_reader::reader::should_skip_package_layer_handling(
            &manifest_reader::reader::BlockMetadata {
                hide_source,
                timeout: None,
            },
            package_name,
        )
}

pub(crate) fn package_scope_enable_layer(hide_source: bool, package_name: Option<&str>) -> bool {
    should_enable_package_layer(hide_source, package_name, layer::feature_enabled())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_packages_disable_package_layer_even_when_feature_is_enabled() {
        assert!(!should_enable_package_layer(
            false,
            Some("@connector/demo"),
            true
        ));
    }

    #[test]
    fn hide_source_packages_disable_package_layer_even_when_feature_is_enabled() {
        assert!(!should_enable_package_layer(
            true,
            Some("regular-pkg"),
            true
        ));
    }

    #[test]
    fn regular_packages_still_enable_package_layer_when_feature_is_enabled() {
        assert!(should_enable_package_layer(false, Some("pkg_a"), true));
    }

    #[test]
    fn connector_prefix_must_match_scope_boundary() {
        assert!(should_enable_package_layer(
            false,
            Some("@connectorx/demo"),
            true
        ));
    }
}
