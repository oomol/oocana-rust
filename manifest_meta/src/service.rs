use manifest_reader::manifest::{self, InputHandles, OutputHandles, ServiceExecutorOptions};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

type BlockName = String;

#[derive(Debug, Clone)]
pub struct ServiceBlock {
    pub name: BlockName,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub service_path: Option<PathBuf>,
    pub service_executor: Arc<Option<ServiceExecutorOptions>>,
    pub package_path: Option<PathBuf>,
}

impl ServiceBlock {
    pub fn from_manifest(
        manifest: manifest::ServiceBlock, service_path: PathBuf,
        executor: Arc<Option<ServiceExecutorOptions>>, package_path: Option<PathBuf>,
    ) -> Self {
        let manifest::ServiceBlock {
            name,
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            name,
            inputs_def: inputs_def,
            outputs_def: outputs_def,
            service_path: Some(service_path),
            service_executor: executor,
            package_path,
        }
    }

    pub fn dir(&self) -> String {
        match &self.service_path {
            Some(path) => path.parent().unwrap().to_str().unwrap().to_owned(),
            None => ".".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Service {
    pub blocks: Option<HashMap<BlockName, Arc<ServiceBlock>>>,
    pub path: Option<PathBuf>,
    pub executor: Arc<Option<ServiceExecutorOptions>>,
    pub package_path: Option<PathBuf>,
}

impl Service {
    pub fn from_manifest(
        manifest: manifest::Service, service_path: PathBuf, package_path: Option<PathBuf>,
    ) -> Self {
        let manifest::Service { blocks, executor } = manifest;

        let executor = Arc::new(executor);

        let blocks = blocks.map(|blocks| {
            blocks
                .into_iter()
                .map(|block| {
                    let block = ServiceBlock::from_manifest(
                        block,
                        service_path.clone(),
                        Arc::clone(&executor),
                        package_path.clone(),
                    );
                    (block.name.to_owned(), Arc::new(block))
                })
                .collect::<HashMap<_, _>>()
        });

        Service {
            blocks,
            executor,
            path: Some(service_path),
            package_path,
        }
    }
}
