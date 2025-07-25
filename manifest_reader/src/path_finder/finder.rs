use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use version_compare::{compare, Cmp};

use utils::error::Result;

use crate::reader::read_package;

use super::block::{
    find_flow_block, find_slot_flow, find_task_block, SlotBlockManifestParams,
    SubflowBlockManifestParams, TaskBlockManifestParams,
};
use super::package::find_package_file;
use super::service::{find_service, ServiceManifestParams};

#[derive(Debug, Clone)]
pub struct BlockPathFinder {
    base_dir: PathBuf,
    pub search_paths: Arc<Vec<PathBuf>>,
    cache: HashMap<String, PathBuf>,
    /// Since package directories are named either `<pkg_name>-<version>` or `<pkg_name>`,
    /// this field stores a map from `pkg_name` to `version` for the first case.
    /// This allows for easy reconstruction of the directory name.
    /// If a directory is simply named `<pkg_name>`, it is not included here.
    pub pkg_version: HashMap<String, String>,
}

// TODO: cache pkg store paths result, only update working_dir
fn collect_latest_pkg_version(
    _working_dir: &PathBuf,
    pkg_store_paths: &Option<Vec<PathBuf>>,
) -> HashMap<String, String> {
    let mut pkg_version = HashMap::new();

    let search_paths = pkg_store_paths.clone().unwrap_or_default();

    for path in search_paths {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.filter_map(Result::ok) {
                let sub_path = entry.path();
                if sub_path.is_dir() {
                    if let Some(ref pkg_path) = find_package_file(&sub_path) {
                        if let Ok(pkg) = read_package(pkg_path) {
                            let pkg_name_without_version = if let Some(name) = pkg.name {
                                name
                            } else {
                                pkg_path
                                    .parent()
                                    .and_then(|n| n.file_name())
                                    .and_then(|n| n.to_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| "".to_string())
                                    .strip_suffix(pkg.version.as_deref().unwrap_or(""))
                                    .unwrap_or("")
                                    .to_string()
                            };

                            if sub_path.file_name().is_some_and(|f| {
                                f.to_str() == Some(pkg_name_without_version.as_str())
                            }) {
                                // skip the directory that matches the package name without version.
                                continue;
                            }

                            pkg_version
                                .entry(pkg_name_without_version)
                                .and_modify(|v: &mut String| {
                                    if let Some(version) = &pkg.version {
                                        if matches!(compare(version, v.as_str()), Ok(Cmp::Gt)) {
                                            *v = version.clone();
                                        }
                                    }
                                })
                                .or_insert_with(|| pkg.version.unwrap_or_default());
                        }
                    }
                }
            }
        }
    }

    tracing::debug!("Collected package versions: {:?}", pkg_version);

    pkg_version
}

impl BlockPathFinder {
    pub fn new<P: Into<PathBuf>>(base_dir: P, search_paths: Option<Vec<PathBuf>>) -> Self {
        let base_dir = base_dir.into();

        let mut pkg_versions = collect_latest_pkg_version(&base_dir, &search_paths);

        let specified_version_package = find_package_file(&base_dir)
            .and_then(|pkg_path| read_package(pkg_path).ok())
            .and_then(|pkg| pkg.dependencies)
            .unwrap_or_default();

        // use the specified version package if it exists
        for (name, version) in specified_version_package.iter() {
            pkg_versions.insert(name.clone(), version.clone());
        }

        Self {
            base_dir,
            cache: HashMap::new(),
            search_paths: Arc::new(search_paths.unwrap_or_default()),
            pkg_version: pkg_versions,
        }
    }

    pub fn subflow<P: Into<PathBuf>>(&self, flow_path: P) -> Self {
        let flow_path: PathBuf = flow_path.into();
        let working_dir = flow_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut pkg_versions = collect_latest_pkg_version(
            &working_dir,
            &Some(self.search_paths.iter().cloned().collect()),
        );

        // subflow should be in a/b/c/flows/flow1/flow.oo.yaml. package.oo.yaml is in a/b/c.
        let specified_version_package = flow_path
            .parent()
            .and_then(|f| f.parent())
            .and_then(|f| f.parent())
            .and_then(find_package_file)
            .and_then(|pkg_path| read_package(pkg_path).ok())
            .and_then(|pkg| pkg.dependencies)
            .unwrap_or_default();

        for (name, version) in specified_version_package.iter() {
            pkg_versions.insert(name.clone(), version.clone());
        }

        Self {
            base_dir: working_dir,
            cache: HashMap::new(),
            search_paths: Arc::clone(&self.search_paths),
            pkg_version: pkg_versions,
        }
    }

    pub fn find_package_file_path(&self, pkg_name: &str) -> Result<PathBuf> {
        let version = self.pkg_version.get(pkg_name);

        let pkg_directory = if let Some(version) = version {
            format!("{}-{}", pkg_name, version)
        } else {
            pkg_name.to_owned()
        };

        let package_dir = {
            let mut path = None;
            for search_path in self.search_paths.iter() {
                let pkg_path = search_path.join(&pkg_directory);
                if pkg_path.exists() {
                    path = Some(pkg_path);
                    break;
                }
            }

            let fallback_pkg_path = self.base_dir.join(&pkg_directory);
            if fallback_pkg_path.exists() {
                path = Some(fallback_pkg_path);
            }

            path
        };

        package_dir
            .and_then(|p| find_package_file(&p))
            .ok_or_else(|| {
                utils::error::Error::new(&format!(
                    "Package {} not found. Search paths: {}",
                    pkg_name,
                    self.search_paths
                        .iter()
                        .map(|p| p.to_str().unwrap_or(""))
                        .collect::<Vec<&str>>()
                        .join(", ")
                ))
            })
    }

    pub fn find_flow_block_path(&mut self, flow_name: &str) -> Result<PathBuf> {
        if let Some(flow_path) = self.cache.get(&format!("flow-{}", flow_name)) {
            return Ok(flow_path.to_owned());
        }

        let flow_path = find_flow_block(SubflowBlockManifestParams {
            value: flow_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(format!("flow-{}", flow_name), flow_path.to_owned());

        Ok(flow_path)
    }

    pub fn find_task_block_path(&mut self, task_name: &str) -> Result<PathBuf> {
        if let Some(task_path) = self.cache.get(&format!("task-{}", task_name)) {
            return Ok(task_path.to_owned());
        }

        let task_path = find_task_block(TaskBlockManifestParams {
            value: task_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(format!("task-{}", task_name), task_path.to_owned());

        Ok(task_path)
    }

    pub fn find_slot_slotflow_path(&mut self, slot_name: &str) -> Result<PathBuf> {
        if let Some(slot_path) = self.cache.get(&format!("slot-{}", slot_name)) {
            return Ok(slot_path.to_owned());
        }

        let slot_path = find_slot_flow(SlotBlockManifestParams {
            value: slot_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(format!("slot-{}", slot_name), slot_path.to_owned());

        Ok(slot_path)
    }

    pub fn find_service_block(&mut self, block_in_service: &str) -> Result<PathBuf> {
        if let Some(service_path) = self.cache.get(&format!("service-{}", block_in_service)) {
            return Ok(service_path.to_owned());
        }

        let service_path = find_service(ServiceManifestParams {
            value: block_in_service,
            base_dir: &self.base_dir,
            block_search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache.insert(
            format!("service-{}", block_in_service),
            service_path.to_owned(),
        );

        Ok(service_path)
    }
}
