use std::{path::PathBuf, sync::Arc};

use manifest_reader::manifest::{self, InputHandles, OutputHandles};

use crate::TaskBlockExecutor;

#[derive(Debug, Clone)]
pub struct TaskBlock {
    pub description: Option<String>,
    pub executor: Arc<TaskBlockExecutor>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    // None means this task block is inline script block
    pub path: Option<PathBuf>,
    pub additional_inputs: bool,
    pub additional_outputs: bool,
    // TODO: package_path is not reliable, it should be removed. use block type instead.
    pub package_path: Option<PathBuf>,
}

impl TaskBlock {
    pub fn executor_entry(&self) -> Option<&str> {
        self.executor.entry()
    }

    fn is_script_block(&self) -> bool {
        self.path.is_none()
    }

    pub fn block_dir(&self) -> Option<PathBuf> {
        if self.is_script_block() {
            return self.package_path.to_owned();
        }

        if let Some(path) = self.path.as_ref() {
            path.parent().map(|parent| parent.to_path_buf())
        } else {
            None
        }
    }

    pub fn path_str(&self) -> Option<String> {
        self.path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
    }
}

impl TaskBlock {
    pub fn from_manifest(
        manifest: manifest::TaskBlock,
        path: Option<PathBuf>,
        package: Option<PathBuf>,
    ) -> Self {
        let manifest::TaskBlock {
            executor,
            inputs_def,
            outputs_def,
            additional_inputs,
            additional_outputs,
            description,
        } = manifest;

        Self {
            executor,
            inputs_def,
            description,
            outputs_def,
            path,
            package_path: package,
            additional_inputs,
            additional_outputs,
        }
    }
}
