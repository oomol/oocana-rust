use manifest_reader::block_manifest_reader::{
    self, resolve_flow_block, resolve_slot_block, resolve_task_block,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use utils::error::Result;

use crate::{Block, FlowBlock, SlotBlock, TaskBlock};

pub type BlockName = String;
pub type BlockPath = PathBuf;
pub type BlockSearchPaths = Vec<PathBuf>;

pub struct BlockReader {
    flow_cache: Option<HashMap<BlockPath, Arc<FlowBlock>>>,
    task_cache: Option<HashMap<BlockPath, Arc<TaskBlock>>>,
    slot_cache: Option<HashMap<BlockPath, Arc<SlotBlock>>>,
}

impl BlockReader {
    pub fn new() -> Self {
        Self {
            flow_cache: None,
            task_cache: None,
            slot_cache: None,
        }
    }

    pub fn resolve_flow_block(
        &mut self, flow_name: &str, resolver: &mut BlockPathResolver,
    ) -> Result<Arc<FlowBlock>> {
        let flow_path = resolver.resolve_flow_block(flow_name)?;

        self.read_flow_block(&flow_path, resolver)
    }

    pub fn resolve_task_node_block(
        &mut self, task_node_block: block_manifest_reader::node::TaskNodeBlock,
        resolver: &mut BlockPathResolver,
    ) -> Result<Arc<TaskBlock>> {
        match task_node_block {
            block_manifest_reader::node::TaskNodeBlock::File(file) => {
                self.read_task_block(&resolver.resolve_task_block(&file)?)
            }
            block_manifest_reader::node::TaskNodeBlock::Inline(block) => {
                let task_block = TaskBlock::from_manifest(block, None);
                Ok(Arc::new(task_block))
            }
        }
    }

    pub fn resolve_slot_node_block(
        &mut self, slot_node_block: block_manifest_reader::node::SlotNodeBlock,
        resolver: &mut BlockPathResolver,
    ) -> Result<Arc<SlotBlock>> {
        match slot_node_block {
            block_manifest_reader::node::SlotNodeBlock::File(file) => {
                self.read_slot_block(&resolver.resolve_slot_block(&file)?)
            }
            block_manifest_reader::node::SlotNodeBlock::Inline(block) => {
                let slot_block = SlotBlock::from_manifest(block, None);
                Ok(Arc::new(slot_block))
            }
        }
    }

    pub fn resolve_block(
        &mut self, block_name: &str, resolver: &mut BlockPathResolver,
    ) -> Result<Block> {
        let flow_path = resolver.resolve_flow_block(block_name);

        if let Ok(flow_path) = flow_path {
            match self.read_flow_block(&flow_path, resolver) {
                Ok(flow) => return Ok(Block::Flow(flow)),
                Err(err) => {
                    if flow_path.ends_with("block.oo.yaml") || flow_path.ends_with("block.oo.yml") {
                        return Err(err);
                    }
                }
            }
        }

        let task_path = resolver.resolve_task_block(block_name);

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

        Err(utils::error::Error::new(&format!(
            "Block {} not found. Search paths: {}",
            block_name,
            resolver
                .block_search_paths
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
            block_manifest_reader::read_task_block(task_path)?,
            Some(task_path.to_owned()),
        ));

        let task_cache = self.task_cache.get_or_insert_with(HashMap::new);
        task_cache.insert(task_path.to_owned(), Arc::clone(&task));

        Ok(task)
    }

    fn read_flow_block(
        &mut self, flow_path: &Path, resolver: &mut BlockPathResolver,
    ) -> Result<Arc<FlowBlock>> {
        if let Some(flow_cache) = &self.flow_cache {
            if let Some(flow) = flow_cache.get(flow_path) {
                return Ok(Arc::clone(flow));
            }
        }

        let flow = Arc::new(FlowBlock::from_manifest(
            block_manifest_reader::read_flow_block(flow_path)?,
            flow_path.to_owned(),
            self,
            resolver.subflow(flow_path),
        )?);

        let flow_cache = self.flow_cache.get_or_insert_with(HashMap::new);
        flow_cache.insert(flow_path.to_owned(), Arc::clone(&flow));

        Ok(flow)
    }

    fn read_slot_block(&mut self, slot_path: &Path) -> Result<Arc<SlotBlock>> {
        if let Some(slot_cache) = &self.slot_cache {
            if let Some(slot) = slot_cache.get(slot_path) {
                return Ok(Arc::clone(slot));
            }
        }

        let slot = Arc::new(SlotBlock::from_manifest(
            block_manifest_reader::read_slot_block(slot_path)?,
            Some(slot_path.to_owned()),
        ));

        let slot_cache = self.slot_cache.get_or_insert_with(HashMap::new);
        slot_cache.insert(slot_path.to_owned(), Arc::clone(&slot));

        Ok(slot)
    }
}

pub struct BlockPathResolver {
    base_dir: PathBuf,
    block_search_paths: Arc<BlockSearchPaths>,
    cache: HashMap<BlockName, BlockPath>,
}

impl BlockPathResolver {
    pub fn new(base_dir: PathBuf, block_search_paths: Option<String>) -> Self {
        let block_search_paths: BlockSearchPaths = block_search_paths
            .map(|paths| {
                paths
                    .split(",")
                    .map(|s| Path::new(s).to_path_buf())
                    .collect()
            })
            .unwrap_or_default();

        Self {
            base_dir,
            cache: HashMap::new(),
            block_search_paths: Arc::new(block_search_paths),
        }
    }

    pub fn subflow(&self, flow_path: &Path) -> Self {
        Self {
            base_dir: flow_path.parent().unwrap().to_path_buf(),
            cache: HashMap::new(),
            block_search_paths: Arc::clone(&self.block_search_paths),
        }
    }

    pub fn resolve_flow_block(&mut self, flow_name: &str) -> Result<BlockPath> {
        if let Some(flow_path) = self.cache.get(flow_name) {
            return Ok(flow_path.to_owned());
        }

        let flow_path = resolve_flow_block(flow_name, &self.base_dir, &self.block_search_paths)?;

        self.cache
            .insert(flow_name.to_owned(), flow_path.to_owned());

        Ok(flow_path)
    }

    pub fn resolve_task_block(&mut self, task_name: &str) -> Result<BlockPath> {
        if let Some(task_path) = self.cache.get(task_name) {
            return Ok(task_path.to_owned());
        }

        let task_path = resolve_task_block(task_name, &self.base_dir, &self.block_search_paths)?;

        self.cache
            .insert(task_name.to_owned(), task_path.to_owned());

        Ok(task_path)
    }

    pub fn resolve_slot_block(&mut self, slot_name: &str) -> Result<BlockPath> {
        if let Some(slot_path) = self.cache.get(slot_name) {
            return Ok(slot_path.to_owned());
        }

        let slot_path = resolve_slot_block(slot_name, &self.base_dir, &self.block_search_paths)?;

        self.cache
            .insert(slot_name.to_owned(), slot_path.to_owned());

        Ok(slot_path)
    }
}
