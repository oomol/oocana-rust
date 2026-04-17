mod cli;
mod external_layer_store;
mod injection_layer;
mod injection_store;
mod layer;
mod layer_settings;
mod ovmlayer;
mod package_layer;
mod package_store;
mod runtime_layer;

use std::process::Command;

pub use external_layer_store::{
    ExternalLayerStatus, ExternalLayerStore, create_external_layer, delete_external_layer,
    external_layer_status, get_external_layer, list_external_layers, load_external_store,
};
pub use ovmlayer::BindPath;
pub use package_layer::{import_package_layer, move_package_layer};
pub use package_store::{
    PackageLayerStatus, delete_all_layer_data, delete_package_layer, get_or_create_package_layer,
    list_package_layers, package_layer_status,
};
pub use runtime_layer::{InjectionParams, RuntimeLayer, create_runtime_layer};

use crate::ovmlayer::is_root;

#[cfg(target_os = "linux")]
fn has_ovmlayer_binary() -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join("ovmlayer");
                candidate.is_file()
                    && std::fs::metadata(&candidate)
                        .map(|m| m.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn has_ovmlayer_binary() -> bool {
    false
}

pub fn feature_enabled() -> bool {
    if !cfg!(target_os = "linux") || !has_ovmlayer_binary() {
        return false;
    }

    if is_root() {
        let mut cmd = Command::new("ovmlayer");
        cmd.arg("test");
        cmd.arg("system");
        cli::exec(cmd).is_ok()
    } else {
        let mut cmd = Command::new("sudo");
        cmd.arg("-E");
        cmd.arg("ovmlayer");
        cmd.arg("test");
        cmd.arg("system");
        cli::exec(cmd).is_ok()
    }
}

// 自己转换，避免空格。同时这个函数也可以用于其他地方
pub fn convert_to_script(cmd: &Vec<&str>) -> String {
    let mut exec_string: String = String::from("");
    for (i, cmd) in cmd.iter().enumerate() {
        exec_string = if i == 0 {
            cmd.to_string()
        } else {
            format!("{exec_string} '{cmd}'")
        }
    }
    exec_string
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_feature_enabled() {
        #[cfg(not(target_os = "linux"))]
        {
            use super::feature_enabled;
            assert!(!feature_enabled());
        }
    }
}
