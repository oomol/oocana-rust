use std::{collections::HashMap, fmt, process::Command};

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
    UnusedLayers,
}

pub fn list_layer_cmd(t: Option<LayerType>) -> Command {
    let mut binding = ovmlayer_bin();
    let list_type = match t {
        Some(LayerType::Layers) => vec!["layers"],
        Some(LayerType::UsedLayers) => vec!["layers", "--used"],
        Some(LayerType::UnusedLayers) => vec!["layers", "--unused"],
        None => vec!["layers"],
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
        options.push("-l");
        options.push(layer);
    }
    options.push("-m");
    options.push(merge_point);
    binding.args(&options);
    binding
}

pub fn unmerge_cmd(merge_point: &str) -> Command {
    let mut binding = ovmlayer_bin();
    binding.args(&["unmerge", merge_point]);
    binding
}

#[derive(Debug, Clone)]
pub struct BindPath {
    pub src: String,
    pub dst: String,
    pub permission: Permission,
    pub bind_option: BindOption,
}

impl BindPath {
    pub fn new(src: &str, dst: &str, readonly: bool, recursive: bool) -> Self {
        let permission = if readonly {
            Permission::Readonly
        } else {
            Permission::ReadWrite
        };
        let bind_option = if recursive {
            BindOption::Recursive
        } else {
            BindOption::NonRecursive
        };
        BindPath {
            src: src.to_string(),
            dst: dst.to_string(),
            permission,
            bind_option,
        }
    }
}

impl TryFrom<&str> for BindPath {
    type Error = String;

    fn try_from(path: &str) -> Result<Self, String> {
        let parts: Vec<&str> = path.split(',').collect();

        let mut src = None;
        let mut dst = None;
        let mut readonly = None;
        let mut recursive = None;
        for part in &parts {
            if part.starts_with("src=") {
                src = Some(part[4..].to_string());
            } else if part.starts_with("dst=") {
                dst = Some(part[4..].to_string());
            } else if *part == "ro" && readonly.is_none() {
                readonly = Some(true)
            } else if *part == "rw" && readonly.is_none() {
                readonly = Some(false);
            } else if *part == "recursive" && recursive.is_none() {
                recursive = Some(true);
            } else if *part == "nonrecursive" && recursive.is_none() {
                recursive = Some(false);
            } else {
                return Err(format!("Invalid BindPath format: {}", path));
            }
        }

        if let (Some(src), Some(dst), readonly, recursive) = (
            src,
            dst,
            readonly.unwrap_or(false),
            recursive.unwrap_or(false),
        ) {
            Ok(BindPath::new(&src, &dst, readonly, recursive))
        } else {
            Err(format!(
                "Invalid BindPath format: {}. Missing src or dst",
                path
            ))
        }
    }
}

impl fmt::Display for BindPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "type=bind,src={},dst={},{},{}",
            self.src, self.dst, self.permission, self.bind_option
        )
    }
}

#[derive(Debug, Clone)]
pub enum Permission {
    Readonly,
    ReadWrite,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Permission::Readonly => write!(f, "ro"),
            Permission::ReadWrite => write!(f, "rw"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum BindOption {
    Recursive,
    NonRecursive,
}

impl fmt::Display for BindOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BindOption::Recursive => write!(f, "recursive"),
            BindOption::NonRecursive => write!(f, "nonrecursive"),
        }
    }
}

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
/// let mut cmd = run_cmd("merge_point", &Vec[], &None););
/// cmd.arg("shell.sh"); // 只能再传入一个参数，这个参数会作为 Command 执行，后续再传入的参数，都是第一个参数执行时的参数内容（以$1, $2, $3...的形式传入），可以参考 zsh -c 的文档。
/// cmd.output().unwrap();
/// ```
pub fn run_cmd(
    merge_point: &str,
    mount_paths: &[BindPath],
    work_dir: &Option<String>,
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
) -> Command {
    let mut binding = ovmlayer_bin();
    let mut options = vec![format!("run"), format!("--all-devices")];

    for bind_path in mount_paths {
        options.push(format!("--mount={}", bind_path));
    }

    if let Some(work_dir) = work_dir {
        options.push(format!("--workdir={}", work_dir).to_string());
    }

    for (env_key, env_value) in envs {
        options.push(format!("--env"));
        options.push(format!("{}={}", env_key, env_value));
    }
    if let Some(env_file) = env_file {
        options.push(format!("--env-file={}", env_file));
    }

    let merged_point = format!("--merged-point={}", merge_point);
    options.push(merged_point);
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
        format!("cp"),
        format!("--mode"),
        format!("host2layer"),
        format!("{src}"),
        format!("{layer}:{dest}",),
    ];
    binding.args(&options);
    binding
}

/// dest must be directory not filepath
/// if src is file, it will be copy to dest directory, the target file is <dest>/<file_name of src>
/// if src is directory, it will be copy to dest directory the target directory is <dest>/<dir_name of src>
pub fn cp_to_merge_point(merge_point: &str, src: &str, dest: &str) -> Command {
    let mut binding = ovmlayer_bin();
    let options = vec![
        format!("cp"),
        format!("--mode"),
        format!("host2merged"),
        format!("{src}"),
        format!("{merge_point}:{dest}",),
    ];
    binding.args(&options);
    binding
}
