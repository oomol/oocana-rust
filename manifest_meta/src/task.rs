use std::path::PathBuf;

use manifest_reader::manifest::{self, InputHandles, OutputHandles};

use crate::TaskBlockExecutor;

#[derive(Debug, Clone)]
pub struct TaskBlock {
    pub description: Option<String>,
    pub executor: Option<TaskBlockExecutor>,
    pub inputs_def: Option<InputHandles>,
    pub outputs_def: Option<OutputHandles>,
    /// block.oo.[yml|yaml] 的路径；如果是 inline block，这个字段为空。
    pub path: Option<PathBuf>,
    pub path_str: Option<String>,
    pub additional_inputs: bool,
    pub additional_outputs: bool,
    // TODO: package_path is not reliable, it should be removed. use block type instead.
    pub package_path: Option<PathBuf>,
}

impl TaskBlock {
    pub fn executor_entry(&self) -> Option<&str> {
        self.executor.as_ref().and_then(|executor| executor.entry())
    }

    // script 小脚本，首先是 TaskNodeBlock File 类型。但是这里没办法自己判断。
    fn is_script_block(&self) -> bool {
        self.executor
            .as_ref()
            .map(|executor| executor.is_script())
            .unwrap_or(false)
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
            path_str: path.as_ref().map(|path| path.to_string_lossy().to_string()),
            path,
            package_path: package,
            additional_inputs,
            additional_outputs,
        }
    }
}
