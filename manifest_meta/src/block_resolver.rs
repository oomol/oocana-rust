use manifest_reader::{manifest, path_finder::BlockPathFinder, reader::read_task_block};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use utils::error::Result;

use crate::service::ServiceBlock;
use crate::{flow_resolver, service_resolver, Block, Service, SlotBlock, SubflowBlock, TaskBlock};
pub type BlockPath = PathBuf;

pub struct BlockResolver {
    flow_cache: Option<HashMap<BlockPath, Arc<SubflowBlock>>>,
    task_cache: Option<HashMap<BlockPath, Arc<TaskBlock>>>,
    service_cache: Option<HashMap<BlockPath, Service>>,
}

impl BlockResolver {
    pub fn new() -> Self {
        Self {
            flow_cache: None,
            task_cache: None,
            service_cache: None,
        }
    }

    pub fn resolve_flow_block(
        &mut self,
        flow_name: &str,
        path_finder: &mut BlockPathFinder,
    ) -> Result<Arc<SubflowBlock>> {
        let flow_path = path_finder.find_flow_block_path(flow_name)?;

        self.read_flow_block(&flow_path, path_finder)
    }

    pub fn resolve_task_node_block(
        &mut self,
        task_node_block: manifest::TaskNodeBlock,
        path_finder: &mut BlockPathFinder,
        injection_package_dir: Option<PathBuf>,
    ) -> Result<Arc<TaskBlock>> {
        match task_node_block {
            manifest::TaskNodeBlock::File(file) => {
                self.read_task_block(&path_finder.find_task_block_path(&file)?)
            }
            manifest::TaskNodeBlock::Inline(block) => {
                let task_block = TaskBlock::from_manifest(block, None, injection_package_dir);
                Ok(Arc::new(task_block))
            }
        }
    }

    pub fn resolve_slot_node_block(
        &mut self,
        slot_node_block: manifest::SlotNodeBlock,
    ) -> Result<Arc<SlotBlock>> {
        match slot_node_block {
            manifest::SlotNodeBlock::Inline(block) => {
                let slot_block = SlotBlock::from_manifest(block, None, None);
                Ok(Arc::new(slot_block))
            }
        }
    }

    pub fn resolve_service_node_block(
        &mut self,
        service_node_block: String,
        finder: &mut BlockPathFinder,
    ) -> Result<Arc<ServiceBlock>> {
        let service_path = finder.find_service_block(&service_node_block)?;
        let block_name = service_node_block.split("::").last().unwrap();
        self.read_service_block(&service_path, block_name)
    }

    pub fn resolve_block(
        &mut self,
        block_name: &str,
        finder: &mut BlockPathFinder,
    ) -> Result<Block> {
        let flow_path = finder.find_flow_block_path(block_name);

        if let Ok(flow_path) = flow_path {
            match self.read_flow_block(&flow_path, finder) {
                Ok(flow) => return Ok(Block::Flow(flow)),
                Err(err) => {
                    if flow_path.ends_with("block.oo.yaml") || flow_path.ends_with("block.oo.yml") {
                        return Err(err);
                    }
                }
            }
        }

        let task_path = finder.find_task_block_path(block_name);

        if let Ok(task_path) = task_path {
            match self.read_task_block(&task_path) {
                Ok(task) => {
                    return Ok(Block::Task(task));
                }
                Err(err) => {
                    if task_path.ends_with("block.oo.yaml") || task_path.ends_with("block.oo.yml") {
                        return Err(err);
                    }
                }
            }
        }

        let service_path = finder.find_service_block(block_name);
        if let Ok(service_path) = service_path {
            match self.read_service_block(&service_path, block_name) {
                Ok(service) => {
                    return Ok(Block::Service(service));
                }
                Err(err) => {
                    if service_path.ends_with("service.oo.yaml")
                        || service_path.ends_with("service.oo.yml")
                    {
                        return Err(err);
                    }
                }
            }
        }

        Err(utils::error::Error::new(&format!(
            "Block {} not found. Search paths: {}",
            block_name,
            finder
                .search_paths
                .iter()
                .map(|p| p.to_str().unwrap_or(""))
                .collect::<Vec<&str>>()
                .join(", ")
        )))
    }

    fn read_task_block(&mut self, task_path: &Path) -> Result<Arc<TaskBlock>> {
        if let Some(task_cache) = &self.task_cache {
            if let Some(task) = task_cache.get(task_path) {
                return Ok(Arc::clone(task));
            }
        }

        let task = Arc::new(TaskBlock::from_manifest(
            read_task_block(task_path)?,
            Some(task_path.to_owned()),
            package_path(task_path).ok(),
        ));

        let task_cache = self.task_cache.get_or_insert_with(HashMap::new);
        task_cache.insert(task_path.to_owned(), Arc::clone(&task));

        Ok(task)
    }

    fn read_flow_block(
        &mut self,
        flow_path: &Path,
        resolver: &mut BlockPathFinder,
    ) -> Result<Arc<SubflowBlock>> {
        if let Some(flow_cache) = &self.flow_cache {
            if let Some(flow) = flow_cache.get(flow_path) {
                return Ok(Arc::clone(flow));
            }
        }

        let flow = Arc::new(flow_resolver::read_flow(flow_path, self, resolver)?);

        let flow_cache = self.flow_cache.get_or_insert_with(HashMap::new);
        flow_cache.insert(flow_path.to_owned(), Arc::clone(&flow));

        Ok(flow)
    }

    fn read_service(&mut self, service_path: &Path) -> Result<&Service> {
        let service_cache = self.service_cache.get_or_insert_with(HashMap::new);

        if service_cache.contains_key(service_path) {
            Ok(service_cache.get(service_path).unwrap())
        } else {
            let package = package_path(service_path).ok();
            let service = service_resolver::read_service(service_path, package)?;
            service_cache.insert(service_path.to_owned(), service);
            Ok(&service_cache[service_path])
        }
    }

    fn read_service_block(
        &mut self,
        service_path: &Path,
        block_name: &str,
    ) -> Result<Arc<ServiceBlock>> {
        let service = self.read_service(service_path)?;

        let block = service
            .blocks
            .as_ref()
            .and_then(|m| m.get(block_name).cloned())
            .ok_or_else(|| {
                utils::error::Error::new(&format!(
                    "Block {} not found in service {}",
                    block_name,
                    service_path.to_str().unwrap_or("")
                ))
            })?;

        Ok(block)
    }
}

// a/b/tasks/<task_name>/block.oo.yaml -> a/b
// a/b/flows/<flow_name>/flow.oo.yaml -> a/b
// a/b/services/<service_name>/service.oo.yaml -> a/b
// a/b/slots/<slot_name>/slot.oo.yaml -> a/b
pub fn package_path(path: &Path) -> Result<PathBuf> {
    path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .map(|p| p.to_owned())
        .ok_or_else(|| utils::error::Error::new("Failed to resolve task package name"))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_task_package() {
        let task_path = Path::new("a/b/tasks/task_name/block.oo.yaml");
        assert_eq!(package_path(task_path).unwrap(), PathBuf::from("a/b"));
    }
}
