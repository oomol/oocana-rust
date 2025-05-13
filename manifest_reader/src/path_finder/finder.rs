use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
    pub pkg_version: HashMap<String, String>,
}

impl BlockPathFinder {
    pub fn new<P: Into<PathBuf>>(base_dir: P, search_paths: Option<Vec<PathBuf>>) -> Self {
        let base_dir = base_dir.into();

        let pkg_version = find_package_file(&base_dir)
            .and_then(|pkg_path| read_package(pkg_path).ok())
            .and_then(|pkg| pkg.dependencies)
            .unwrap_or_default();

        Self {
            base_dir,
            cache: HashMap::new(),
            search_paths: Arc::new(search_paths.unwrap_or_default()),
            pkg_version,
        }
    }

    pub fn subflow<P: Into<PathBuf>>(&self, flow_path: P) -> Self {
        let flow_path: PathBuf = flow_path.into();

        // subflow should be in a/b/c/flows/flow1/flow.oo.yaml. package.oo.yaml is in a/b/c.
        let pkg_version = flow_path
            .parent()
            .and_then(|f| f.parent())
            .and_then(|f| f.parent())
            .and_then(find_package_file)
            .and_then(|pkg_path| read_package(pkg_path).ok())
            .and_then(|pkg| pkg.dependencies)
            .unwrap_or_default();

        Self {
            base_dir: flow_path.parent().unwrap().to_path_buf(),
            cache: HashMap::new(),
            search_paths: Arc::clone(&self.search_paths),
            pkg_version,
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
        if let Some(flow_path) = self.cache.get(flow_name) {
            return Ok(flow_path.to_owned());
        }

        let flow_path = find_flow_block(SubflowBlockManifestParams {
            value: flow_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(flow_name.to_owned(), flow_path.to_owned());

        Ok(flow_path)
    }

    pub fn find_task_block_path(&mut self, task_name: &str) -> Result<PathBuf> {
        if let Some(task_path) = self.cache.get(task_name) {
            return Ok(task_path.to_owned());
        }

        let task_path = find_task_block(TaskBlockManifestParams {
            value: task_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(task_name.to_owned(), task_path.to_owned());

        Ok(task_path)
    }

    pub fn find_slot_slotflow_path(&mut self, slot_name: &str) -> Result<PathBuf> {
        if let Some(slot_path) = self.cache.get(slot_name) {
            return Ok(slot_path.to_owned());
        }

        let slot_path = find_slot_flow(SlotBlockManifestParams {
            value: slot_name,
            base_dir: &self.base_dir,
            search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(slot_name.to_owned(), slot_path.to_owned());

        Ok(slot_path)
    }

    pub fn find_service_block(&mut self, block_in_service: &str) -> Result<PathBuf> {
        if let Some(service_path) = self.cache.get(block_in_service) {
            return Ok(service_path.to_owned());
        }

        let service_path = find_service(ServiceManifestParams {
            value: block_in_service,
            base_dir: &self.base_dir,
            block_search_paths: &self.search_paths,
            pkg_version: &self.pkg_version,
        })?;

        self.cache
            .insert(block_in_service.to_owned(), service_path.to_owned());

        Ok(service_path)
    }
}
