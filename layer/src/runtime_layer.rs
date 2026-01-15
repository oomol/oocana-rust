use crate::injection_layer::InjectionLayer;
use crate::injection_store::get_injection_layer;
use crate::layer::{
    create_tmp_layer, delete_layer, merge_layer, random_merge_point, random_name, unmerge,
};
use crate::ovmlayer::{self, BindPath};
use crate::package_layer::{PackageLayer, CACHE_DIR};
use crate::package_store::get_or_create_package_layer;
use crate::registry_layer_store::get_or_create_registry_layer;
use std::collections::HashMap;
use std::env::temp_dir;
use std::fmt::Debug;
use std::fs::metadata;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::vec;
use tracing::{debug, info, instrument, warn};
use utils::error::Result;

/// package runtime layer is package layer + tmp layer. The tmp layer is used to capture all package runtime IO changes and it will be deleted after the package layer is dropped.
#[derive(Debug)]
pub struct RuntimeLayer {
    /// The layer that is capture all package runtime IO changes. currently it is a tmp layer and will be deleted after the package layer is dropped.
    tmp_layer: String,
    /// The merge point should be unmerged after the package layer is dropped. so it shouldn't be serialized.
    merge_point: String,
    pub version: Option<String>,
    /// The layers that package layer is based on.
    pub layers: Option<Vec<String>>,
    /// The layer that bootstrap script is executed in.
    pub bootstrap_layer: Option<String>,
    pub package_path: PathBuf,
    pub extra_bind_paths: Vec<BindPath>,
    /// 目前没有用到，只是单纯存储了 injection layer
    pub extra_layers: Option<Vec<String>>,
}

pub fn create_runtime_layer(
    package: &str,
    bind_paths: &[BindPath],
    envs: &HashMap<String, String>,
    env_file: &Option<String>,
    package_name: Option<&str>,
    version: Option<&str>,
) -> Result<RuntimeLayer> {
    let layer: std::result::Result<PackageLayer, utils::error::Error> =
        match (package_name, version) {
            (Some(pkg_name), Some(ver)) => {
                match get_or_create_registry_layer(
                    pkg_name, ver, package, bind_paths, envs, env_file,
                ) {
                    Ok(layer) => {
                        info!("runtime layer from registry store: {}@{}", pkg_name, ver);
                        Ok(layer)
                    }
                    Err(e) => {
                        info!(
                            "get registry layer failed: {:?}, fallback to package layer",
                            e
                        );
                        match get_or_create_package_layer(package, bind_paths, envs, env_file) {
                            Ok(layer) => {
                                info!("runtime layer from package store: {}", package);
                                Ok(layer)
                            }
                            Err(e) => Err(e),
                        }
                    }
                }
            }
            _ => match get_or_create_package_layer(package, bind_paths, envs, env_file) {
                Ok(layer) => {
                    info!("runtime layer from package store: {}", package);
                    Ok(layer)
                }
                Err(e) => Err(e),
            },
        };

    match layer {
        Ok(layer) => match create_runtime_layer_from_package_layer(&layer) {
            Ok(mut runtime_layer) => {
                runtime_layer.add_bind_paths(bind_paths);
                Ok(runtime_layer)
            }
            Err(e) => {
                warn!("create runtime package failed: {:?}", e);
                Err(e)
            }
        },
        Err(e) => {
            warn!("get package layer failed: {:?}", e);
            Err(e)
        }
    }
}

pub struct InjectionParams<'a> {
    pub package_path: &'a str,
    pub package_version: &'a str,
    pub scripts: &'a Vec<String>,
    pub flow_path: &'a str,
}

fn create_runtime_layer_from_package_layer(layer: &PackageLayer) -> Result<RuntimeLayer> {
    let layers = if let Some(ref layers) = layer.base_layers {
        let mut layers = layers.clone();
        layers.push(layer.source_layer.clone());
        layers
    } else {
        vec![layer.source_layer.clone()]
    };
    RuntimeLayer::create(
        layer.version.clone(),
        Some(layers),
        layer.bootstrap_layer.clone(),
        layer.package_path.clone(),
    )
}

impl RuntimeLayer {
    pub fn tmp_layer(&self) -> &str {
        &self.tmp_layer
    }

    pub fn add_bind_paths(&mut self, bind_paths: &[BindPath]) {
        for b in bind_paths {
            if metadata(&b.src).is_ok() {
                self.extra_bind_paths.push(b.clone());
            } else {
                warn!("add_bind_paths skip paths {:?} which does not exist", b.src);
            }
        }
    }

    /// this Command is immutable. because lifetime limit, we can't add more arguments to it.
    #[instrument(skip_all)]
    pub fn run_command(
        &self,
        script: &str,
        envs: &HashMap<String, String>,
        env_file: &Option<String>,
    ) -> Command {
        let mut bind_paths: Vec<BindPath> = vec![];

        for b in &self.extra_bind_paths {
            if metadata(&b.src).is_ok() {
                bind_paths.push(b.clone());
            } else {
                warn!("bind paths {:?} is not exist", b.src);
            }
        }

        let work_dir = self.package_path.to_string_lossy().to_string();
        let mut cmd = ovmlayer::run_cmd(
            &self.merge_point,
            &bind_paths,
            &Some(work_dir),
            envs,
            env_file,
        );
        cmd.arg(script);
        cmd
    }

    #[instrument(skip_all)]
    pub fn create<P: Into<PathBuf> + Debug>(
        version: Option<String>,
        layers: Option<Vec<String>>,
        bootstrap_layer: Option<String>,
        package_path: P,
    ) -> Result<Self> {
        let mut merge_layers = if let Some(ref layers) = layers {
            layers.clone()
        } else {
            vec![]
        };

        if let Some(ref layer) = bootstrap_layer {
            merge_layers.push(layer.clone());
        }

        let tmp_layer = create_tmp_layer()?;
        merge_layers.push(tmp_layer.clone());

        let merge_point = random_merge_point();

        merge_layer(&merge_layers, &merge_point)?;

        Ok(Self {
            version,
            layers,
            tmp_layer,
            merge_point,
            bootstrap_layer,
            package_path: package_path.into(),
            extra_bind_paths: vec![],
            extra_layers: None,
        })
    }

    #[instrument(skip_all)]
    pub fn inject_runtime_layer(&mut self, params: InjectionParams) -> Result<()> {
        let InjectionParams {
            package_path,
            package_version,
            scripts,
            flow_path,
        } = params;

        let injection_layer_name =
            if let Some(injection_layer) = get_injection_layer(flow_path, package_path) {
                if injection_layer.package_version == package_version
                    && injection_layer.is_equal_scripts(scripts.clone())
                {
                    debug!(
                        "injection layer {} is matched, skip creation step",
                        injection_layer.layer_name
                    );
                    injection_layer.layer_name.clone()
                } else {
                    delete_layer(&injection_layer.layer_name)?;
                    let new_injection_layer = InjectionLayer::new(
                        flow_path.to_owned(),
                        scripts.clone(),
                        package_path.to_owned(),
                        package_version.to_owned(),
                    );
                    new_injection_layer.save_to_store()?;
                    info!(
                        "injection layer {} is not matched, delete old and create {}",
                        injection_layer.layer_name, new_injection_layer.layer_name
                    );
                    new_injection_layer.layer_name.clone()
                }
            } else {
                let new_injection_layer = InjectionLayer::new(
                    flow_path.to_owned(),
                    scripts.clone(),
                    package_path.to_owned(),
                    package_version.to_owned(),
                );
                new_injection_layer.save_to_store()?;
                info!(
                    "injection layer not exist, create {}",
                    new_injection_layer.layer_name
                );
                new_injection_layer.layer_name.clone()
            };

        self.run_injection_scripts(
            injection_layer_name,
            scripts.clone(),
            package_path,
            &HashMap::default(), // TODO: implement envs for injection runtime layer
            &None,               // TODO: implement env_file for injection runtime layer
        )?;

        Ok(())
    }

    #[instrument(skip_all)]
    fn run_injection_scripts(
        &mut self,
        extra_layer: String,
        scripts: Vec<String>,
        package_path: &str,
        envs: &HashMap<String, String>,
        env_file: &Option<String>,
    ) -> Result<()> {
        let mut bind_paths: Vec<BindPath> = vec![];

        for cache in &*CACHE_DIR {
            if metadata(cache).is_err() {
                tracing::warn!("cache path: {cache:?} not exist. skip this bind path");
                continue;
            }
            bind_paths.push(BindPath::new(cache, cache, false, false));
        }

        let pkg_path = self.package_path.to_string_lossy().to_string();

        let mut script_runtime_layers = self.layers.clone().unwrap_or_default();

        if let Some(ref bootstrap_layer) = self.bootstrap_layer {
            script_runtime_layers.push(bootstrap_layer.clone());
        }

        script_runtime_layers.push(extra_layer.clone());

        let script_run_merge_point = random_merge_point();
        merge_layer(&script_runtime_layers, &script_run_merge_point)?;

        let mut scripts_files = vec![];
        for script in scripts {
            let script_filename = random_name("script") + ".sh";
            let script_path = temp_dir()
                .join(script_filename.clone())
                .to_string_lossy()
                .to_string();

            let mut script_run_path = PathBuf::from(package_path);
            script_run_path.push(&script_filename);

            let script_run_path_str = script_run_path.to_str().unwrap();

            std::fs::write(
                &script_path,
                format!("#! /usr/bin/env zsh\nset -eo pipefail\n{script}",),
            )?;

            let mut perms = std::fs::metadata(&script_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script_path, perms)?;

            bind_paths.push(BindPath::new(
                &script_path,
                script_run_path_str,
                false,
                false,
            ));
            scripts_files.push(script_run_path_str.to_string());
        }

        let pkg_path_arg = Some(pkg_path.clone());

        for script in scripts_files {
            let mut cmd = ovmlayer::run_cmd(
                &script_run_merge_point,
                &bind_paths,
                &pkg_path_arg,
                envs,
                env_file,
            );
            cmd.arg(&script);
            let child = cmd.spawn()?;

            debug!("cmd: {:?}", cmd);
            let output = child.wait_with_output()?;
            if output.status.success() {
                tracing::info!("run script success");
            } else {
                tracing::error!(
                    "run script failed: {:?} err: {:?} out: {:?}",
                    script,
                    String::from_utf8_lossy(&output.stderr),
                    String::from_utf8_lossy(&output.stdout)
                );
                return Err("run script failed".into());
            }
        }

        unmerge(&script_run_merge_point)?;

        // unmerge 旧的 merge point，把 extra layer 塞到 tmp_layer 前面，重新用这个 merge point 名称。
        unmerge(&self.merge_point)?;
        script_runtime_layers.push(self.tmp_layer.clone());
        merge_layer(&script_runtime_layers, &self.merge_point)?;

        if let Some(ref mut extra_layers) = self.extra_layers {
            extra_layers.push(extra_layer);
        } else {
            self.extra_layers = Some(vec![extra_layer]);
        }

        Ok(())
    }
}

impl Drop for RuntimeLayer {
    #[instrument(skip_all)]
    fn drop(&mut self) {
        let merge_point = &self.merge_point;
        let _ = unmerge(merge_point);
        let _ = delete_layer(&self.tmp_layer);
    }
}
