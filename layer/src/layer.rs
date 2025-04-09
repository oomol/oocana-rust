use std::collections::HashMap;
use std::env::temp_dir;
use std::io::BufRead;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;
use std::thread;
use tracing::instrument;
use utils::logger::{STDERR_TARGET, STDOUT_TARGET};

use crate::cli::{self, exec};
use crate::ovmlayer::{
    cp_to_merge_point, create_layer_cmd, delete_all_layer_and_merge_point_cmd, delete_layer_cmd,
    export_layer_cmd, import_layer_cmd, list_layer_cmd, merge_cmd, run_cmd, unmerge_cmd, BindPath,
    LayerType,
};
use utils::error::{Error, Result};

#[instrument(skip_all)]
pub fn create_layer(name: &str) -> Result<()> {
    let cmd = create_layer_cmd(name);
    cli::exec(cmd).map(|_o| ())
}

#[instrument(skip_all)]
pub fn merge_layer(layers: &Vec<String>, merge_point: &str) -> Result<()> {
    let base_rootfs = crate::layer_settings::load_base_rootfs()?;

    let mut merged_layers = base_rootfs.clone();
    merged_layers.extend(layers.iter().cloned());
    let cmd = merge_cmd(&merged_layers, merge_point);
    cli::exec_without_output(cmd)
}

/// t: None, will use LayerType::Layers
#[instrument(skip_all)]
pub fn list_layers(t: Option<LayerType>) -> Result<Vec<String>> {
    let cmd = list_layer_cmd(t);
    let output = cli::exec(cmd)?;

    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| Error::from(format!("Failed to convert stdout to string: {err}")))?;
    let layers: Vec<String> = stdout.lines().map(|line| line.to_string()).collect();
    Ok(layers)
}

#[allow(unused)]
#[instrument(skip_all)]
pub fn export_layer(name: &str, dest: &str) -> Result<String> {
    let cmd = export_layer_cmd(name, dest);
    let output = cli::exec(cmd)?;
    let output = String::from_utf8(output.stdout)
        .map_err(|err| Error::from(format!("Failed to convert stdout to string: {err}")))?;
    // get last line
    let result = output
        .lines()
        .last()
        .ok_or_else(|| Error::from("Failed to get export layer path"))?;
    Ok(result.trim().to_string())
}

#[allow(unused)]
#[instrument(skip_all)]
pub fn import_layer(file: &str) -> Result<()> {
    let cmd = import_layer_cmd(file);
    cli::exec_without_output(cmd)
}

#[instrument(skip_all)]
pub fn delete_layer(name: &str) -> Result<()> {
    let cmd = delete_layer_cmd(name);
    cli::exec_without_output(cmd)
}

#[instrument(skip_all)]
pub fn delete_all_layer_data() -> Result<()> {
    let cmd = delete_all_layer_and_merge_point_cmd();
    cli::exec_without_output(cmd)
}

#[instrument(skip_all)]
pub fn unmerge(merge_point: &str) -> Result<()> {
    let cmd = unmerge_cmd(merge_point);
    cli::exec_without_output(cmd)
}

pub fn convert_script_to_shell(script: &str) -> Result<(String, String)> {
    let filename = random_name("script") + ".sh";
    let content = format!("#! /usr/bin/env zsh\nset -eo pipefail\n{}", script);
    let script_path = temp_dir().join(&filename).to_string_lossy().to_string();
    std::fs::write(&script_path, content)?;

    let metadata = std::fs::metadata(&script_path)?;

    let mut perms = metadata.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script_path, perms)?;

    Ok((script_path, filename))
}

/// bind HashMap's key is the source path, value is the target path
///
/// run command is executed in merged point. All IO changed will saved in the top layer
/// this function will automatically unmerge the merge point.
#[instrument(skip_all)]
pub fn run_script_unmerge(
    layers: &Vec<String>,
    bind: &[BindPath],
    work_dir: &Option<String>,
    script: &str,
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
) -> Result<()> {
    let merge_point = &random_merge_point();
    merge_layer(&layers, merge_point)?;

    let (file_path, script_filename) = convert_script_to_shell(script)?;
    let script_target_path = {
        if let Some(work_dir) = work_dir {
            work_dir.clone() + &script_filename
        } else {
            file_path.clone()
        }
    };
    let cp_cmd = cp_to_merge_point(&merge_point, &file_path, &script_target_path);
    exec(cp_cmd)?;

    let mut cmd = run_cmd(merge_point, &bind, work_dir, envs, env_file);
    cmd.arg(format!("{script_target_path}"));
    tracing::info!("{cmd:?}");

    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    match child {
        Ok(mut child) => {
            let stdout_handler = child.stdout.take().map(|stdout| {
                thread::spawn(move || {
                    let mut reader = std::io::BufReader::new(stdout).lines();
                    while let Some(line) = reader.next() {
                        if let Ok(line) = line {
                            tracing::info!(target: STDOUT_TARGET, "{}", line);
                        }
                    }
                })
            });

            let stderr_handler = child.stderr.take().map(|stderr| {
                thread::spawn(move || {
                    let mut reader = std::io::BufReader::new(stderr).lines();
                    while let Some(line) = reader.next() {
                        if let Ok(line) = line {
                            tracing::info!(target: STDERR_TARGET, "{}", line);
                        }
                    }
                })
            });
            let result = child.wait();
            if let Some(handler) = stdout_handler {
                drop(handler);
            }
            if let Some(handler) = stderr_handler {
                drop(handler);
            }

            match result {
                Ok(output) => {
                    if output.success() {
                        unmerge(merge_point)?;
                        Ok(())
                    } else {
                        unmerge(merge_point)?;
                        Err(Error::from(format!("{script} failed {output:?}")))
                    }
                }
                Err(e) => {
                    unmerge(merge_point)?;
                    Err(Error::from(format!("{script} result error: {e:?}")))
                }
            }
        }
        Err(e) => {
            unmerge(merge_point)?;
            return Err(Error::from(format!("spawn failed {e}")));
        }
    }
}

static TMP_LAYER_PREFIX: &str = "tmp_layer";
static LAYER_PREFIX: &str = "layer";
static MERGE_PREFIX: &str = "merge";

pub fn create_tmp_layer() -> Result<String> {
    let tmp = random_name(TMP_LAYER_PREFIX);
    create_layer(&tmp)?;
    Ok(tmp)
}

pub fn create_random_layer() -> Result<String> {
    let layer = random_layer_name();
    create_layer(&layer)?;
    Ok(layer)
}

pub fn random_name(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        uuid::Uuid::new_v4().to_string().replace("-", "_")
    )
}

fn random_layer_name() -> String {
    random_name(LAYER_PREFIX)
}

pub fn random_merge_point() -> String {
    random_name(MERGE_PREFIX)
}

#[cfg(target_os = "linux")]
#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn test_export_import_layer() {
        use super::*;

        let r = create_random_layer();
        assert!(r.is_ok(), "create layer failed: {:?}", &r.unwrap_err());

        let random_layer = r.unwrap();

        let r = export_layer(&random_layer, "/tmp");
        assert!(r.is_ok(), "export layer failed: {:?}", r.unwrap_err());

        let file_path = r.unwrap();

        fs::metadata(&file_path).expect(&format!("{file_path} not exit"));

        let r = delete_layer(&random_layer);
        assert!(r.is_ok(), "delete layer failed: {:?}", r.unwrap_err());

        let r = import_layer(file_path.as_str());
        assert!(r.is_ok(), "import layer failed: {:?}", r.unwrap_err());

        let list = list_layers(None);
        assert!(list.is_ok(), "list layer failed: {:?}", list.unwrap_err());

        let layers = list.unwrap();
        let layer_name = random_layer;
        assert!(
            layers.contains(&layer_name),
            "import layer failed: {:?}",
            layers
        );
    }

    #[test]
    fn test_layer_operate() {
        use super::*;
        use crate::package_store::clean_layer_not_in_store;

        let r = clean_layer_not_in_store();
        assert!(
            r.is_ok(),
            "clean_layer_not_in_store failed: {:?}",
            r.unwrap_err()
        );

        let r = create_random_layer();
        assert!(r.is_ok(), "create layer failed: {:?}", r.unwrap_err());

        let r = delete_all_layer_data();
        assert!(
            r.is_ok(),
            "delete all layer data failed: {:?}",
            r.unwrap_err()
        );

        let l1 = "l1";
        let r = create_layer(&l1);
        assert!(r.is_ok(), "create layer failed: {:?}", r.unwrap_err());

        let merge_point = "merge_point";
        let layers: Vec<String> = vec![l1.to_owned()];
        let r = merge_layer(&layers, merge_point);
        assert!(r.is_ok(), "merge layer failed: {:?}", r.unwrap_err());

        let r = unmerge(merge_point);
        assert!(r.is_ok(), "unmerge layer failed: {:?}", r.unwrap_err());

        let r = delete_layer(&l1);
        assert!(r.is_ok(), "delete layer failed: {:?}", r.unwrap_err());

        let r = create_tmp_layer();
        assert!(r.is_ok(), "create tmp layer failed: {:?}", r.unwrap_err());

        let lists = list_layers(None);
        assert!(
            lists.is_ok(),
            "list layers failed: {:?}",
            lists.unwrap_err()
        );

        println!("list layers: {:?}", lists.unwrap());

        let lists = list_layers(Some(LayerType::UsedLayers));
        assert!(
            lists.is_ok(),
            "list layers failed: {:?}",
            lists.unwrap_err()
        );

        println!("list layers: {:?}", lists.unwrap());
    }
}
