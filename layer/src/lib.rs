mod cli;
mod injection_layer;
mod injection_store;
mod layer;
mod layer_settings;
mod ovmlayer;
mod package_layer;
mod package_store;
mod runtime_layer;

use std::process::Command;

pub use ovmlayer::BindPath;
pub use package_layer::import_package_layer;
pub use package_store::{
    delete_all_layer_data, delete_package_layer, get_or_create_package_layer, list_package_layers,
    package_layer_status, PackageLayerStatus,
};
pub use runtime_layer::{create_runtime_layer, InjectionParams, RuntimeLayer};
pub fn feature_enabled() -> bool {
    let mut cmd = Command::new("ovmlayer");
    cmd.arg("version");
    cli::exec(cmd).is_ok()
}

// ovmlayer 会根据环境变量来存放日志文件
pub static OVMLAYER_LOG_ENV_KEY: &str = "OVMLAYER_LOG";

// 自己转换，避免空格。同时这个函数也可以用于其他地方
pub fn convert_to_script(cmd: &Vec<&str>) -> String {
    let mut exec_string: String = String::from("");
    for (i, cmd) in cmd.iter().enumerate() {
        exec_string = if i == 0 {
            cmd.to_string()
        } else {
            format!("{} '{}'", exec_string, cmd)
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
            assert_eq!(feature_enabled(), false);
        }
    }
}
