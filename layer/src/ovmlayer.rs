use std::collections::HashMap;
use std::process::Command;

fn ovmlayer_bin() -> Command {
    const BIN: &str = "ovmlayer";
    // ovmlayer 需要 root 权限。
    // ci 环境需要使用 sudo 来获取 root 权限；普通情况下 oocana 执行需要调用方保证 root 权限。
    if std::env::var("CI").is_ok() {
        let mut cmd = Command::new("sudo");
        cmd.arg("-E"); // 保留环境变量。
        cmd.arg(BIN);
        cmd
    } else {
        Command::new(BIN)
    }
}

pub fn create_layer_cmd(name: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["create", name]);
    binding
}

#[allow(unused)]
pub enum LayerType {
    Layers,
    UsedLayers,
    Merged,
}

pub fn list_layer_cmd(t: Option<LayerType>) -> Command {
    let mut binding = ovmlayer_bin();
    let list_type = match t {
        Some(LayerType::Layers) => vec!["--layers"],
        Some(LayerType::UsedLayers) => vec!["--layers", "--used"],
        Some(LayerType::Merged) => vec!["--merged"],
        None => vec!["--layers"],
    };
    binding.arg("list");
    binding.args(&list_type);
    binding
}

pub fn export_layer_cmd(name: &str, dest: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&[
        "export",
        &format!("--layer={name}"),
        &format!("--dest={dest}"),
    ]);
    binding
}

pub fn import_layer_cmd(file: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["import", file]);
    binding
}

pub fn delete_layer_cmd(name: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["delete", name]);
    binding
}

pub fn delete_all_layer_and_merge_point_cmd() -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["delete", "--all"]);
    binding
}

pub fn merge_cmd(layers: &Vec<String>, merge_point: &str) -> Command {
    let mut binding = ovmlayer_bin();
    let mut options = vec!["merge"];
    for layer in layers {
        options.push(layer);
    }
    options.push(":");
    options.push(merge_point);
    binding.args(&options);
    binding
}

pub fn unmerge_cmd(merge_point: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["unmerge", merge_point]);
    binding
}

/// bind: key is the source path, value is the target path
/// run command is executed in merged point. All IO changed will saved in the top layer
///
/// This command need add script shell to execute
///
/// $ Examples
///
/// ```ignore
/// use std::collections::HashMap;
/// use crate::ovmlayer::run_cmd;
///
/// let mut cmd = run_cmd("merge_point", &Some(HashMap::new()));
/// cmd.arg("shell.sh"); // 只能再传入一个参数，这个参数会作为 Command 执行，后续再传入的参数，都是第一个参数执行时的参数内容（以$1, $2, $3...的形式传入），可以参考 zsh -c 的文档。
/// cmd.output().unwrap();
/// ```
pub fn run_cmd(
    merge_point: &str, bind: &Option<HashMap<String, String>>, work_dir: &Option<String>,
) -> Command {
    let mut binding = ovmlayer_bin();
    let mut options = vec!["run".to_string()];

    if let Some(bind) = bind {
        for (key, value) in bind.iter() {
            options.push(format!("--bind={}:{}", key, value));
        }
    }

    if let Some(work_dir) = work_dir {
        options.push(format!("--workdir={}", work_dir).to_string());
    }

    let merged_point = format!("--merged-point={}", merge_point);
    options.push(merged_point);
    options.push("--".to_string());
    // 使用 zsh -i 来加载 zshrc 获取 PATH 环境变量，绕开 sudo 导致的 PATH 改变问题。
    options.push("zsh".to_string());
    options.push("-i".to_string());
    options.push("-c".to_string());
    binding.args(&options);
    binding
}

pub fn cp_to_layer(layer: &str, src: &str, dest: &str) -> Command {
    let mut binding = ovmlayer_bin();
    let options = vec![
        "cp-layer".to_string(),
        src.to_string(),
        format!("{layer}:{dest}",),
    ];
    binding.args(&options);
    binding
}

pub fn cp_to_merge_point(merge_point: &str, src: &str, dest: &str) -> Command {
    let mut binding = ovmlayer_bin();
    let options = vec![
        "cp-merged".to_string(),
        src.to_string(),
        format!("{merge_point}:{dest}",),
    ];
    binding.args(&options);
    binding
}
