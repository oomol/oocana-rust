use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use crate::block;
use crate::{InputHandles, OutputHandles};
use manifest_reader::block_manifest_reader::{self, applet::Applet as AppletManifest};

#[derive(Debug, Clone)]
pub struct AppletExecutorOptions {
    pub name: Option<String>,
    pub entry: Option<String>,
    pub function: Option<String>,
    pub start_at: Option<String>,
    pub stop_at: Option<String>,
    pub keep_alive: Option<u64>,
}

type BlockName = String;

#[derive(Debug, Clone)]
pub struct AppletBlock {
    pub name: BlockName,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    pub applet_path: Option<PathBuf>,
    pub applet_executor: Arc<Option<AppletExecutorOptions>>,
}

impl AppletBlock {
    pub fn from_manifest(
        manifest: block_manifest_reader::block::AppletBlock, applet_path: PathBuf,
        executor: Arc<Option<AppletExecutorOptions>>,
    ) -> Self {
        let block_manifest_reader::block::AppletBlock {
            name,
            inputs_def,
            outputs_def,
        } = manifest;

        Self {
            name,
            inputs_def: block::to_input_handles(inputs_def),
            outputs_def: block::to_output_handles(outputs_def),
            applet_path: Some(applet_path),
            applet_executor: executor,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Applet {
    pub blocks: Option<HashMap<BlockName, Arc<AppletBlock>>>,
    pub path: Option<PathBuf>,
    pub executor: Arc<Option<AppletExecutorOptions>>,
}

impl Applet {
    pub fn from_manifest(manifest: AppletManifest, applet_path: PathBuf) -> Self {
        let AppletManifest { blocks, executor } = manifest;

        let executor = Arc::new(executor.map(|e| AppletExecutorOptions {
            name: e.name,
            entry: e.entry,
            function: e.function,
            start_at: e.start_at,
            stop_at: e.stop_at,
            keep_alive: e.keep_alive,
        }));

        let blocks = blocks.map(|blocks| {
            blocks
                .into_iter()
                .map(|block| {
                    let block = AppletBlock::from_manifest(
                        block,
                        applet_path.clone(),
                        Arc::clone(&executor),
                    );
                    (block.name.to_owned(), Arc::new(block))
                })
                .collect::<HashMap<_, _>>()
        });

        Applet {
            blocks,
            executor,
            path: Some(applet_path),
        }
    }
}
