use async_trait::async_trait;
use flume::{Receiver, Sender};
use layer::{create_runtime_layer, BindPath, InjectionParams, RuntimeLayer};
use port_check::free_local_ipv4_port_in_range;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    default,
    path::PathBuf,
    process,
    sync::{Arc, RwLock},
    vec,
};
use utils::calculate_short_hash;

use job::{BlockInputs, BlockJobStackLevel, JobId, RuntimeScope, SessionId};

use manifest_meta::{
    HandleName, InjectionStore, InputDefPatchMap, InputHandles, JsonValue, NodeId, OutputHandles,
    ServiceExecutorOptions, TaskBlockExecutor,
};
use tokio::io::AsyncBufReadExt;

use tokio::process::Command as tokioCommand;
use tracing::{debug, error, info, instrument, warn};

use utils::error::{Error, Result};

use crate::MessageData;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct RunBlockRequest {
    pub session_id: SessionId,
    pub job_id: JobId,
    pub block: String, // format: `self::<block>` / `<package>::<block>`
    /// new job's job_id
    pub block_job_id: String,
    pub payload: serde_json::Value,
    pub strict: Option<bool>,
    pub stacks: Vec<BlockJobStackLevel>,
    pub request_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct QueryBlockRequest {
    pub session_id: SessionId,
    pub job_id: JobId,
    pub block: String, // format: `self::<block>` / `<package>::<block>`
    pub request_id: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "action")]
pub enum BlockRequest {
    RunBlock(RunBlockRequest),
    QueryBlock(QueryBlockRequest),
    QueryDownstream {
        session_id: SessionId,
        job_id: JobId,
        /// None means return all handle outputs
        outputs: Option<Vec<HandleName>>,
        request_id: String,
    },
}

impl BlockRequest {
    pub fn session_id(&self) -> &SessionId {
        match self {
            BlockRequest::RunBlock(request) => &request.session_id,
            BlockRequest::QueryBlock(request) => &request.session_id,
            BlockRequest::QueryDownstream { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> &JobId {
        match self {
            BlockRequest::RunBlock(request) => &request.job_id,
            BlockRequest::QueryBlock(request) => &request.job_id,
            BlockRequest::QueryDownstream { job_id, .. } => job_id,
        }
    }

    // request_id must exist.
    pub fn request_id(&self) -> &str {
        match self {
            BlockRequest::RunBlock(request) => &request.request_id,
            BlockRequest::QueryBlock(request) => &request.request_id,
            BlockRequest::QueryDownstream { request_id, .. } => request_id,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OutputOptions {
    /// Specifies the destination for block outputs, such as node inputs or flow outputs.
    /// If `None`, outputs are sent to all default destinations.
    pub target: Option<OutputTarget>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OutputTarget {
    pub to_node: Option<Vec<ToNodeInput>>,
    pub to_flow: Option<Vec<ToFlowOutput>>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ToFlowOutput {
    pub output_handle: HandleName,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct ToNodeInput {
    pub node_id: NodeId,
    pub input_handle: HandleName,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ReceiveMessage {
    BlockReady {
        session_id: SessionId,
        job_id: JobId,
    },
    BlockOutput {
        session_id: SessionId,
        job_id: JobId,
        handle: HandleName,
        output: JsonValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        /// if not None, it means this output is only to specific nodes. None means all nodes.
        options: Option<OutputOptions>,
    },
    BlockOutputs {
        session_id: SessionId,
        job_id: JobId,
        outputs: HashMap<HandleName, JsonValue>,
    },
    BlockError {
        session_id: SessionId,
        job_id: JobId,
        error: String,
    },
    BlockProgress {
        session_id: SessionId,
        job_id: JobId,
        /// progress is a float number between 0.0 ~ 100.0
        progress: f32,
    },
    BlockFinished {
        session_id: SessionId,
        job_id: JobId,
        result: Option<HashMap<HandleName, JsonValue>>,
        error: Option<String>,
    },
    ExecutorReady {
        session_id: SessionId,
        executor_name: String,
        package: Option<String>,
        identifier: Option<String>,
    },
    BlockRequest(BlockRequest),
    // --- 以下消息，是通过 scheduler 发送给 subscriber 的消息，而不是 mqtt 消息 --- //
    ExecutorTimeout {
        session_id: SessionId,
        executor_name: String,
        package: Option<String>,
        identifier: Option<String>,
    },
    ExecutorExit {
        session_id: SessionId,
        executor_name: String,
        code: i32,
        reason: Option<String>,
    },
    // --- 以下消息，是其他信息发送的 --- //
    ListenerTimeout {
        session_id: SessionId,
        job_id: JobId,
    },
}

impl ReceiveMessage {
    pub fn session_id(&self) -> &SessionId {
        match self {
            ReceiveMessage::BlockReady { session_id, .. } => session_id,
            ReceiveMessage::BlockOutput { session_id, .. } => session_id,
            ReceiveMessage::BlockOutputs { session_id, .. } => session_id,
            ReceiveMessage::BlockError { session_id, .. } => session_id,
            ReceiveMessage::BlockProgress { session_id, .. } => session_id,
            ReceiveMessage::BlockFinished { session_id, .. } => session_id,
            ReceiveMessage::BlockRequest(block) => block.session_id(),
            ReceiveMessage::ExecutorReady { session_id, .. } => session_id,
            ReceiveMessage::ExecutorExit { session_id, .. } => session_id,
            ReceiveMessage::ExecutorTimeout { session_id, .. } => session_id,
            ReceiveMessage::ListenerTimeout { session_id, .. } => session_id,
        }
    }

    pub fn job_id(&self) -> Option<&JobId> {
        match self {
            ReceiveMessage::BlockReady { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockOutput { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockOutputs { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockError { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockProgress { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockFinished { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockRequest(block) => Some(block.job_id()),
            ReceiveMessage::ExecutorReady { .. } => None,
            ReceiveMessage::ExecutorExit { .. } => None,
            ReceiveMessage::ExecutorTimeout { .. } => None,
            ReceiveMessage::ListenerTimeout { job_id, .. } => Some(job_id),
        }
    }
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ExecutePayload<'a> {
    BlockInputs {
        session_id: &'a SessionId,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        #[serde(skip_serializing_if = "Option::is_none")]
        block_path: &'a Option<String>,
        inputs: Option<&'a BlockInputs>,
        inputs_def: &'a Option<InputHandles>,
        inputs_def_patch: &'a Option<InputDefPatchMap>,
    },
    BlockPayload {
        session_id: &'a SessionId,
        executor_name: &'a str,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        dir: &'a str,
        executor: &'a TaskBlockExecutor,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: &'a Option<OutputHandles>,
        identifier: &'a str,
    },
    ServiceBlockPayload {
        session_id: &'a SessionId,
        job_id: &'a JobId,
        stacks: &'a Vec<BlockJobStackLevel>,
        executor_name: &'a str,
        dir: &'a str,
        block_name: &'a str,
        service_executor: &'a ServiceExecutorOptions,
        #[serde(skip_serializing_if = "Option::is_none")]
        outputs: &'a Option<OutputHandles>,
        service_hash: String,
        identifier: &'a str,
    },
}

// job 和 session id 在发送给 executor 时，是必须的。添加这两个函数是为了保证这两个字段不缺失。
impl ExecutePayload<'_> {
    pub fn job_id(&self) -> &JobId {
        match self {
            ExecutePayload::BlockInputs { job_id, .. } => job_id,
            ExecutePayload::BlockPayload { job_id, .. } => job_id,
            ExecutePayload::ServiceBlockPayload { job_id, .. } => job_id,
        }
    }

    pub fn session_id(&self) -> &SessionId {
        match self {
            ExecutePayload::BlockInputs { session_id, .. } => session_id,
            ExecutePayload::BlockPayload { session_id, .. } => session_id,
            ExecutePayload::ServiceBlockPayload { session_id, .. } => session_id,
        }
    }
}

#[async_trait]
pub trait SchedulerTxImpl {
    // for builtin block, send these block events to broker. make these event is send to broker.
    async fn send_block_event(&self, session_id: &SessionId, data: MessageData);
    async fn send_inputs(&self, job_id: &JobId, data: MessageData);
    async fn run_block(&self, executor_name: &str, data: MessageData);
    async fn respond_block_request(
        &self,
        session_id: &SessionId,
        request_id: &str,
        data: MessageData,
    );
    async fn run_service_block(&self, executor_name: &str, data: MessageData);
    async fn disconnect(&self);
}

#[async_trait]
pub trait SchedulerRxImpl {
    async fn recv(&mut self) -> MessageData;
}

enum SchedulerCommand {
    RegisterSubscriber(JobId, Sender<ReceiveMessage>),
    UnregisterSubscriber(JobId),
    BlockEvent {
        event: ReceiveMessage,
    },
    SendInputs {
        job_id: JobId,
        stacks: Vec<BlockJobStackLevel>,
        block_path: Option<String>,
        inputs: Option<BlockInputs>,
        inputs_def: Option<InputHandles>,
        inputs_def_patch: Option<InputDefPatchMap>,
    },
    BlockRequestResponse {
        session_id: SessionId,
        job_id: JobId,
        error: Option<String>,
        result: Option<serde_json::Value>,
        request_id: String,
    },
    ExecuteBlock {
        job_id: JobId,
        executor_name: String,
        dir: String,
        stacks: Vec<BlockJobStackLevel>,
        outputs: Option<OutputHandles>,
        executor: TaskBlockExecutor,
        injection_store: Option<InjectionStore>,
        scope: RuntimeScope,
        flow: Option<String>,
    },
    ExecuteServiceBlock {
        job_id: JobId,
        executor_name: String,
        dir: String,
        block_name: String,
        service_executor: ServiceExecutorOptions,
        stacks: Vec<BlockJobStackLevel>,
        outputs: Option<OutputHandles>,
        scope: RuntimeScope,
        service_hash: String,
        flow: Option<String>,
    },
    ExecutorExit {
        executor: String,
        code: i32,
        reason: Option<String>,
    },
    SpawnExecutorTimeout {
        executor: String,
        package: Option<String>,
        identifier: Option<String>,
    },
    ReceiveMessage(MessageData),
    Abort,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutorState {
    spawn_state: ExecutorSpawnState,
    pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ExecutorSpawnState {
    #[default]
    None,
    Spawned,  // spawn 但是没有收到 ready 信息
    Ready,    // 收到 ready 信息
    Finished, // executor only finished during session finished. This enum is used to indicate that the executor has finished its work and is no longer running. If this state is reached do not try to send executor spawn timeout.
}

#[derive(Debug, Clone)]
pub struct SchedulerTx {
    tx: Sender<SchedulerCommand>,
    // make default scope always exist
    default_package: Option<String>,
    exclude_packages: Option<Vec<String>>,
    data_dir: String,
}

pub struct BlockResponseParams {
    pub session_id: SessionId,
    pub job_id: JobId,
    pub error: Option<String>,
    pub result: Option<serde_json::Value>,
    pub request_id: String,
}

pub struct InputParams {
    pub job_id: JobId,
    pub stacks: Vec<BlockJobStackLevel>,
    pub block_path: Option<String>,
    pub inputs: Option<BlockInputs>,
    pub inputs_def: Option<InputHandles>,
    pub inputs_def_patch: Option<InputDefPatchMap>,
}

pub struct ExecutorParams<'a> {
    pub executor_name: &'a str,
    pub job_id: JobId,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub dir: String,
    pub executor: &'a TaskBlockExecutor,
    pub outputs: &'a Option<OutputHandles>,
    pub scope: &'a RuntimeScope,
    pub injection_store: &'a Option<InjectionStore>,
    pub flow: &'a Option<String>,
}

pub struct ServiceParams<'a> {
    pub executor_name: &'a str,
    pub block_name: &'a str,
    pub job_id: JobId,
    pub stacks: &'a Vec<BlockJobStackLevel>,
    pub dir: String,
    pub options: &'a ServiceExecutorOptions,
    pub outputs: &'a Option<OutputHandles>,
    pub scope: &'a RuntimeScope,
    pub flow: &'a Option<String>,
}

pub struct ExecutorCheckResult {
    pub executor_state: ExecutorSpawnState,
    pub layer: Option<RuntimeLayer>, // layer is only exist when executor_exist is false
}

pub struct ExecutorCheckParams<'a> {
    pub executor_name: &'a str,
    pub scope: &'a RuntimeScope,
    pub injection_store: &'a Option<InjectionStore>,
    pub flow: &'a Option<String>,
    pub executor_payload: &'a ExecutorParameters,
    pub executor_map: Arc<RwLock<HashMap<String, ExecutorState>>>,
}

fn generate_executor_map_name(executor_name: &str, scope: &RuntimeScope) -> String {
    format!("{}-{}", executor_name, scope.identifier())
}

impl SchedulerTx {
    pub fn send_inputs(&self, params: InputParams) {
        let InputParams {
            job_id,
            stacks,
            block_path,
            inputs,
            inputs_def,
            inputs_def_patch,
        } = params;

        self.tx
            .send(SchedulerCommand::SendInputs {
                job_id,
                stacks,
                block_path,
                inputs,
                inputs_def,
                inputs_def_patch,
            })
            .unwrap();
    }

    pub fn respond_block_request(&self, session_id: &SessionId, params: BlockResponseParams) {
        self.tx
            .send(SchedulerCommand::BlockRequestResponse {
                session_id: session_id.clone(),
                job_id: params.job_id,
                error: params.error,
                result: params.result,
                request_id: params.request_id,
            })
            .unwrap();
    }

    pub fn send_block_event(&self, event: ReceiveMessage) {
        self.tx
            .send(SchedulerCommand::BlockEvent { event })
            .unwrap();
    }

    /// disable layer feature is scope package is in exclude package.
    /// if default package is exist, move to default FIXME: remove default package path, use package from package store.
    pub fn calculate_scope(&self, scope: &RuntimeScope) -> RuntimeScope {
        match self.exclude_packages.as_ref() {
            Some(exclude_packages) => {
                let pkg_str = scope.path().to_string_lossy().to_string();
                if exclude_packages.contains(&pkg_str) {
                    match self.default_package {
                        Some(ref default_package) => RuntimeScope {
                            session_id: scope.session_id.clone(),
                            pkg_name: None,
                            data_dir: self.data_dir.clone(),
                            pkg_root: scope.pkg_root.clone(),
                            path: PathBuf::from(default_package.clone()),
                            node_id: scope.node_id().clone(),
                            enable_layer: false,
                            is_inject: scope.is_inject(),
                        },
                        None => RuntimeScope {
                            session_id: scope.session_id.clone(),
                            pkg_name: None,
                            data_dir: self.data_dir.clone(),
                            pkg_root: scope.pkg_root.clone(),
                            path: scope.path().clone(),
                            node_id: scope.node_id().clone(),
                            enable_layer: false,
                            is_inject: scope.is_inject(),
                        },
                    }
                } else {
                    scope.clone()
                }
            }
            None => scope.clone(),
        }
    }

    #[instrument(skip_all)]
    pub fn send_to_executor(&self, params: ExecutorParams) {
        let ExecutorParams {
            executor_name,
            job_id,
            stacks,
            dir,
            executor,
            outputs,
            scope,
            injection_store,
            flow,
        } = params;

        let scope = self.calculate_scope(scope);
        self.tx
            .send(SchedulerCommand::ExecuteBlock {
                job_id,
                executor_name: executor_name.to_owned(),
                dir,
                stacks: stacks.clone(),
                scope,
                outputs: outputs.clone(),
                executor: executor.clone(),
                injection_store: injection_store.clone(),
                flow: flow.clone(),
            })
            .unwrap();
    }

    pub fn send_to_service(&self, params: ServiceParams) {
        let ServiceParams {
            executor_name,
            block_name,
            job_id,
            stacks,
            dir,
            options,
            outputs,
            scope,
            flow,
        } = params;

        let scope = self.calculate_scope(scope);

        self.tx
            .send(SchedulerCommand::ExecuteServiceBlock {
                job_id,
                executor_name: executor_name.to_owned(),
                dir: dir.to_owned(),
                block_name: block_name.to_owned(),
                service_executor: options.clone(),
                scope,
                stacks: stacks.clone(),
                outputs: outputs.clone(),
                service_hash: calculate_short_hash(&dir, 16),
                flow: flow.clone(),
            })
            .unwrap();
    }

    pub fn register_subscriber(&self, job_id: JobId, sender: Sender<ReceiveMessage>) {
        self.tx
            .send(SchedulerCommand::RegisterSubscriber(job_id, sender))
            .unwrap()
    }

    pub fn unregister_subscriber(&self, job_id: JobId) {
        self.tx
            .send(SchedulerCommand::UnregisterSubscriber(job_id))
            .unwrap()
    }

    pub fn abort(&self) {
        self.tx.send(SchedulerCommand::Abort).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerRx<TT, TR>
where
    TT: SchedulerTxImpl,
    TR: SchedulerRxImpl,
{
    impl_tx: TT,
    impl_rx: TR,
    executor_map: Arc<RwLock<HashMap<String, ExecutorState>>>,
    executor_payload: ExecutorParameters,
    tx: Sender<SchedulerCommand>,
    rx: Receiver<SchedulerCommand>,
}

fn spawn_executor(
    executor: &str,
    layer: Option<RuntimeLayer>,
    scope: &RuntimeScope,
    executor_map: Arc<RwLock<HashMap<String, ExecutorState>>>,
    executor_payload: ExecutorParameters,
    tx: Sender<SchedulerCommand>,
) -> Result<()> {
    let executor_map_name = generate_executor_map_name(executor, scope);
    let mut write_map = executor_map.write().unwrap();
    info!("spawn executor {}", executor_map_name);
    if write_map.get(&executor_map_name).is_some() {
        debug!(
            "{} is already in executor_map. skipping spawn executor",
            executor_map_name
        );
        return Result::Ok(());
    }

    write_map.insert(
        executor_map_name.to_owned(),
        ExecutorState {
            spawn_state: ExecutorSpawnState::Spawned,
            pid: None,
        },
    );
    drop(write_map);

    let ExecutorParameters {
        session_id,
        addr,
        session_dir,
        pass_through_env_keys,
        bind_paths: _bind_paths,
        env_file,
        tmp_dir,
        debug,
        wait_for_client,
    } = &executor_payload;

    // 后面加 -executor 尾缀是一种隐式约定。例如：如果 executor 是 "python"，那么实际上会执行 python-executor。
    // 目前约定 executor 执行文件在 PATH 环境变量中。
    let executor_bin = executor.to_owned() + "-executor";

    let mut executor_package: Option<String> = None;

    let identifier = scope.identifier();
    let scope_package = scope.path().to_string_lossy().to_string();

    // this dir won't pass to executor. the executor generate tmp pkg dir by package parameter.
    let tmp_pkg_dir = if let Some(pkg) = scope.path().file_name() {
        tmp_dir.join(pkg)
    } else {
        tmp_dir.join("workspace")
    };

    if !tmp_pkg_dir.exists() {
        std::fs::create_dir_all(&tmp_pkg_dir).unwrap_or_else(|e| {
            tracing::warn!(
                "Failed to create tmp_pkg_dir: {:?}, error: {}",
                tmp_pkg_dir,
                e
            );
        });
    }

    let tmp_dir = tmp_dir.to_string_lossy().to_string();

    let debug_parameters: Vec<String> = if *debug {
        match executor {
            "nodejs" => {
                let port = free_local_ipv4_port_in_range(9230..=9999)
                    .map(|p| format!("{}", p))
                    .ok_or("Failed to get free port from 9230 to 9999".to_string())?;
                match *wait_for_client {
                    true => vec![
                        "--enable-source-maps".to_owned(),
                        format!("--inspect-wait={}", port),
                    ],
                    false => vec!["--enable-source-maps".to_owned(), port],
                }
            } // nodejs accept SIGUSR1 to debugging. just --enable-source-maps is for source map and typescript debugging support.
            "python" => {
                let port = free_local_ipv4_port_in_range(5678..=9000)
                    .map(|p| format!("{}", p))
                    .ok_or("Failed to get free port from 5678 to 9000".to_string())?;
                match *wait_for_client {
                    true => vec![
                        "--debug-port".to_owned(),
                        port,
                        "--wait-for-client".to_owned(),
                    ],
                    false => vec!["--debug-port".to_owned(), port],
                }
            }
            _ => vec![],
        }
    } else {
        vec![]
    };

    let mut envs: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| key.starts_with("OOMOL_") || pass_through_env_keys.contains(key))
        .collect();

    envs.insert("IS_FORKED".to_string(), "1".to_string());

    tracing::debug!("pass through these env keys: {:?}", envs.keys());

    let mut command = if let Some(ref pkg_layer) = layer {
        let package_path_str = pkg_layer.package_path.to_string_lossy();

        envs.insert("OOCANA_PKG_DIR".to_string(), scope.data_dir.clone());

        let mut exec_form_cmd: Vec<&str> = vec![
            &executor_bin,
            "--session-id",
            session_id,
            "--address",
            addr,
            "--session-dir",
            session_dir,
            "--tmp-dir",
            tmp_dir.as_str(),
        ];

        if !identifier.is_empty() {
            exec_form_cmd.push("--identifier");
            exec_form_cmd.push(&identifier);
        }

        exec_form_cmd.push("--package");
        exec_form_cmd.push(&scope_package);

        for p in debug_parameters.iter() {
            exec_form_cmd.push(p);
        }

        executor_package = Some(package_path_str.to_string());

        let log_filename = format!("ovmlayer-{}-{}.log", executor_bin, identifier);

        let log_dir = utils::logger::logger_dir();
        envs.insert(
            layer::OVMLAYER_LOG_ENV_KEY.to_owned(),
            log_dir.join(&log_filename).to_string_lossy().to_string(),
        );

        let script_str = layer::convert_to_script(&exec_form_cmd);

        pkg_layer.run_command(&script_str, &envs, env_file)
    } else {
        envs.insert("OOCANA_PKG_DIR".to_string(), scope.data_dir.clone());
        for (key, value) in utils::env::load_env_from_file(env_file) {
            if envs.contains_key(&key) {
                // TODO: consider whether to skip the env key or not.
                warn!("env key {} is already in envs, skip", key);
            } else {
                envs.insert(key.to_owned(), value.to_owned());
            }
        }

        let mut args = vec![
            "--session-id",
            session_id,
            "--address",
            addr,
            "--session-dir",
            session_dir,
            "--tmp-dir",
            tmp_dir.as_str(),
        ];

        if !identifier.is_empty() {
            args.push("--identifier");
            args.push(&identifier);
        }

        for p in debug_parameters.iter() {
            args.push(p);
        }

        args.push("--package");
        args.push(&scope_package);

        let mut cmd = process::Command::new(&executor_bin);
        cmd.args(args);
        cmd
    };

    command
        .envs(envs)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    info!("spawn executor: {:?}", command);
    let mut tokio_command = tokioCommand::from(command);

    let child = tokio_command.spawn();

    let executor_map_clone = executor_map.clone();
    let txx = tx.clone();

    let executor_bin_clone = executor_bin.clone();
    let executor_map_name_clone = executor_map_name.clone();
    let identifier_clone = identifier.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        {
            let read_map = executor_map_clone.read().unwrap();
            let executor_state = read_map
                .get(&executor_map_name_clone)
                .cloned()
                .unwrap_or_default();
            if executor_state.spawn_state != ExecutorSpawnState::Spawned {
                return;
            }
            txx.send(SchedulerCommand::SpawnExecutorTimeout {
                executor: executor_bin_clone,
                package: executor_package,
                identifier: Some(identifier_clone),
            })
            .unwrap();
        }
    });

    match child {
        Ok(mut ch) => {
            let pid = ch.id();
            let mut map = executor_map.write().unwrap();
            map.insert(
                executor_map_name.clone(),
                ExecutorState {
                    spawn_state: ExecutorSpawnState::Spawned,
                    pid,
                },
            );
            drop(map);

            if let Some(stdout) = ch.stdout.take() {
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                let executor_bin_clone = executor_bin.clone();
                let identifier_clone = identifier.clone();
                tokio::spawn(async move {
                    while let Ok(Some(line)) = reader.next_line().await {
                        debug!(
                            "{} ({}) stdout: {}",
                            executor_bin_clone, identifier_clone, line
                        );
                    }
                });
            }

            if let Some(stderr) = ch.stderr.take() {
                let mut reader = tokio::io::BufReader::new(stderr).lines();
                let executor_bin_clone = executor_bin.clone();

                tokio::spawn(async move {
                    while let Ok(Some(line)) = reader.next_line().await {
                        debug!("{} ({}) stderr: {}", executor_bin_clone, identifier, line);
                    }
                });
            }
            let executor_bin_clone = executor_bin;
            let executor_map_clone = executor_map.clone();
            let executor_map_name_clone = executor_map_name.clone();

            tokio::spawn(async move {
                let status = ch.wait().await;
                let mut write_map = executor_map_clone.write().unwrap();
                write_map.insert(
                    executor_map_name_clone.clone(),
                    ExecutorState {
                        spawn_state: ExecutorSpawnState::Finished,
                        pid: None,
                    },
                );
                drop(write_map);
                if let Some(layer) = layer {
                    drop(layer);
                }
                match status {
                    // the time maybe after scheduler shutdown, send to tx will fail
                    Ok(status) => {
                        let code = status.code().unwrap_or(-1);
                        info!("{executor_bin_clone} ({executor_map_name_clone}) {pid:?} exit with {code}");
                        if !status.success() {
                            let _ = tx.send(SchedulerCommand::ExecutorExit {
                                executor: executor_bin_clone.clone(),
                                code,
                                reason: None,
                            });
                        }
                    }
                    Err(e) => {
                        error!("wait {} error: {:?}", executor_bin_clone, e);
                        let _ = tx.send(SchedulerCommand::ExecutorExit {
                            executor: executor_bin_clone.clone(),
                            code: -1,
                            reason: Some(format!("{}", e)),
                        });
                    }
                }
            });
            info!("{} spawn success", executor_map_name);
            Result::Ok(())
        }
        Err(e) => {
            let mut write_map = executor_map.write().unwrap();
            write_map.insert(
                executor_map_name,
                ExecutorState {
                    spawn_state: ExecutorSpawnState::None,
                    pid: None,
                },
            );
            drop(write_map);
            if let Some(layer) = layer {
                drop(layer);
            }
            let message = format!("Failed to spawn {}. {}", executor_bin, e);
            error!(message);
            Result::Err(Error::new(&message))
        }
    }
}

fn query_executor_state(params: ExecutorCheckParams) -> Result<ExecutorCheckResult> {
    let ExecutorCheckParams {
        executor_name,
        scope,
        injection_store,
        executor_map,
        executor_payload,
        flow,
    } = params;

    let executor_map_name = generate_executor_map_name(executor_name, scope);

    let executor_state = {
        let read_map = executor_map
            .read()
            .map_err(|_| Error::new("Failed to acquire read lock"))?;
        read_map
            .get(&executor_map_name)
            .cloned()
            .unwrap_or_default()
            .spawn_state
    };

    if executor_state != ExecutorSpawnState::None {
        return Ok(ExecutorCheckResult {
            executor_state,
            layer: None,
        });
    } else if !scope.need_layer() {
        let pkg_dir = PathBuf::from(&scope.data_dir);
        if !pkg_dir.exists() {
            std::fs::create_dir_all(&pkg_dir).unwrap_or_else(|e| {
                tracing::warn!("Failed to create pkg_dir: {:?}, error: {}", pkg_dir, e);
            });
        }

        return Ok(ExecutorCheckResult {
            executor_state,
            layer: None,
        });
    }

    let layer = if scope.need_layer() {
        let mut bind_paths = executor_payload.bind_paths.clone();
        let pkg = scope.path();

        if let Some(store) = injection_store {
            let target = manifest_meta::InjectionTarget::Package(scope.path().to_owned());
            if let Some(meta) = store.get(&target) {
                tracing::info!(
                    "scope layer need create with injection. target: {:?}",
                    target
                );
                for node in meta.nodes.iter() {
                    bind_paths.push(BindPath::new(
                        node.absolute_entry
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default()
                            .as_ref(),
                        &format!(
                            "{}/{}",
                            pkg.to_string_lossy(),
                            node.relative_entry
                                .parent()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default()
                        ),
                        false,
                        false,
                    ));
                }
            }
        }

        let pkg_dir = PathBuf::from(&scope.data_dir);
        if !pkg_dir.exists() {
            std::fs::create_dir_all(&pkg_dir).unwrap_or_else(|e| {
                tracing::warn!("Failed to create pkg_dir: {:?}, error: {}", pkg_dir, e);
            });
        }

        bind_paths.push(BindPath::new(
            pkg_dir.to_string_lossy().as_ref(),
            pkg_dir.to_string_lossy().as_ref(),
            false,
            false,
        ));

        let path_str = pkg.to_string_lossy().to_string();
        let mut runtime_layer = create_runtime_layer(
            &path_str,
            &bind_paths,
            &HashMap::default(),
            &executor_payload.env_file,
        )?;

        if let Some(store) = injection_store {
            let target = manifest_meta::InjectionTarget::Package(scope.path().to_owned());
            if let Some(meta) = store.get(&target) {
                let scripts = meta.scripts.clone().unwrap_or_default();

                let result = runtime_layer.inject_runtime_layer(InjectionParams {
                    package_version: &meta.package_version,
                    package_path: &path_str,
                    scripts: &scripts,
                    flow: flow.as_ref().unwrap_or(&"".to_string()),
                });

                if let Err(e) = result {
                    return Result::Err(e);
                }
            }
        }

        Some(runtime_layer)
    } else {
        let pkg_dir = PathBuf::from(&scope.data_dir);
        if !pkg_dir.exists() {
            std::fs::create_dir_all(&pkg_dir).unwrap_or_else(|e| {
                tracing::warn!("Failed to create pkg_dir: {:?}, error: {}", pkg_dir, e);
            });
        }

        info!("scope {:?} does not need layer", scope);
        None
    };

    Ok(ExecutorCheckResult {
        executor_state,
        layer,
    })
}

impl<TT, TR> SchedulerRx<TT, TR>
where
    TT: SchedulerTxImpl + Send + 'static,
    TR: SchedulerRxImpl + Send + 'static,
{
    pub fn event_loop(self) -> tokio::task::JoinHandle<()> {
        let mut subscribers = HashMap::new();
        let Self {
            tx,
            rx,
            executor_map,
            executor_payload,
            impl_tx,
            mut impl_rx,
        } = self;

        let session_id = executor_payload.session_id.clone();
        let tx_clone = tx.clone();

        tokio::spawn(async move {
            loop {
                let data = impl_rx.recv().await;
                // if data is empty, it means the impl_rx is closed.
                if data.is_empty() {
                    break;
                }
                tx_clone
                    .send(SchedulerCommand::ReceiveMessage(data))
                    .unwrap();
            }
        });

        tokio::spawn(async move {
            loop {
                match rx.recv_async().await {
                    Ok(SchedulerCommand::RegisterSubscriber(job_id, sender)) => {
                        debug_assert!(!subscribers.contains_key(&job_id));
                        subscribers.insert(job_id, sender);
                    }
                    Ok(SchedulerCommand::UnregisterSubscriber(job_id)) => {
                        subscribers.remove(&job_id);
                    }
                    Ok(SchedulerCommand::BlockEvent { event }) => {
                        if matches!(
                            event,
                            ReceiveMessage::BlockReady { .. }
                                | ReceiveMessage::BlockFinished { .. }
                                | ReceiveMessage::BlockOutput { .. }
                                | ReceiveMessage::BlockOutputs { .. }
                                | ReceiveMessage::BlockError { .. }
                        ) {
                            let data = serde_json::to_vec(&event).unwrap();
                            impl_tx.send_block_event(&session_id, data).await;
                        } else {
                            warn!("Received unexpected block event: {:?}", event);
                        }
                    }
                    Ok(SchedulerCommand::SendInputs {
                        job_id,
                        stacks,
                        block_path,
                        inputs,
                        inputs_def,
                        inputs_def_patch,
                    }) => {
                        let data = serde_json::to_vec(&ExecutePayload::BlockInputs {
                            session_id: &session_id,
                            job_id: &job_id,
                            stacks: &stacks,
                            block_path: &block_path,
                            inputs: inputs.as_ref(),
                            inputs_def: &inputs_def,
                            inputs_def_patch: &inputs_def_patch,
                        })
                        .unwrap();
                        impl_tx.send_inputs(&job_id, data).await;
                    }
                    Ok(SchedulerCommand::BlockRequestResponse {
                        session_id,
                        job_id,
                        error,
                        result,
                        request_id,
                    }) => {
                        #[derive(serde::Serialize)]
                        struct BlockResponse {
                            session_id: SessionId,
                            job_id: JobId,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            error: Option<String>,
                            #[serde(skip_serializing_if = "Option::is_none")]
                            result: Option<serde_json::Value>,
                            request_id: String,
                        }

                        let data = serde_json::to_vec(&BlockResponse {
                            session_id: session_id.clone(),
                            job_id,
                            error,
                            result,
                            request_id: request_id.clone(),
                        })
                        .unwrap();
                        impl_tx
                            .respond_block_request(&session_id, &request_id, data)
                            .await;
                    }
                    Ok(SchedulerCommand::ExecuteServiceBlock {
                        job_id,
                        executor_name,
                        dir,
                        block_name,
                        scope,
                        service_executor,
                        stacks,
                        outputs,
                        service_hash,
                        flow,
                    }) => {
                        let result = query_executor_state(ExecutorCheckParams {
                            executor_name: &executor_name,
                            scope: &scope,
                            injection_store: &None,
                            executor_payload: &executor_payload,
                            executor_map: executor_map.clone(),
                            flow: &flow,
                        });

                        if let Err(e) = result {
                            tx.send(SchedulerCommand::ExecutorExit {
                                executor: executor_name.clone(),
                                code: -1,
                                reason: Some(format!("{:?}", e)),
                            })
                            .unwrap();
                            continue;
                        }

                        let ExecutorCheckResult {
                            executor_state,
                            layer,
                            ..
                        } = result.unwrap();

                        if executor_state == ExecutorSpawnState::None {
                            tracing::info!(
                                "run service block with job id {} but need spawning executor {} identifier: {} first",
                                job_id,
                                executor_name,
                                scope.identifier()
                            );
                            let r = spawn_executor(
                                &executor_name,
                                layer,
                                &scope,
                                executor_map.clone(),
                                executor_payload.clone(),
                                tx.clone(),
                            );
                            if let Err(e) = r {
                                tx.send(SchedulerCommand::ExecutorExit {
                                    executor: executor_name.clone(),
                                    code: -1,
                                    reason: Some(format!("{:?}", e)),
                                })
                                .unwrap();
                            }
                        } else {
                            tracing::info!(
                                "run service block with job id {} in executor {} identifier: {}",
                                job_id,
                                executor_name,
                                scope.identifier()
                            );
                            let data = serde_json::to_vec(&ExecutePayload::ServiceBlockPayload {
                                session_id: &session_id,
                                job_id: &job_id,
                                stacks: &stacks,
                                executor_name: &executor_name,
                                dir: &dir,
                                block_name: &block_name,
                                service_executor: &service_executor,
                                outputs: &outputs,
                                service_hash,
                                identifier: &scope.identifier(),
                            })
                            .unwrap();
                            impl_tx.run_service_block(&executor_name, data).await;
                        }
                    }
                    Ok(SchedulerCommand::ExecuteBlock {
                        job_id,
                        executor_name,
                        dir,
                        scope,
                        stacks,
                        outputs,
                        executor,
                        injection_store,
                        flow,
                    }) => {
                        let result = query_executor_state(ExecutorCheckParams {
                            executor_name: &executor_name,
                            scope: &scope,
                            injection_store: &injection_store,
                            executor_payload: &executor_payload,
                            executor_map: executor_map.clone(),
                            flow: &flow,
                        });

                        if let Err(e) = result {
                            let _ = tx.send(SchedulerCommand::ExecutorExit {
                                executor: executor_name.clone(),
                                code: -1,
                                reason: Some(format!("{}", e)),
                            });
                            continue;
                        }

                        let ExecutorCheckResult {
                            executor_state,
                            layer,
                            ..
                        } = result.unwrap();

                        if executor_state == ExecutorSpawnState::None {
                            tracing::info!(
                                "run block with job id {} but need spawning executor {} identifier: {} first",
                                job_id,
                                executor_name,
                                scope.identifier()
                            );
                            let r = spawn_executor(
                                &executor_name,
                                layer,
                                &scope,
                                executor_map.clone(),
                                executor_payload.clone(),
                                tx.clone(),
                            );

                            if let Err(e) = r {
                                tx.send(SchedulerCommand::ExecutorExit {
                                    executor: executor_name.clone(),
                                    code: -1,
                                    reason: Some(format!("{}", e)),
                                })
                                .unwrap();
                            }
                        } else {
                            tracing::info!(
                                "run block with job id {} in executor {} identifier: {}",
                                job_id,
                                executor_name,
                                scope.identifier()
                            );
                            let data = serde_json::to_vec(&ExecutePayload::BlockPayload {
                                session_id: &session_id,
                                executor_name: &executor_name,
                                job_id: &job_id,
                                stacks: &stacks,
                                dir: &dir,
                                executor: &executor,
                                outputs: &outputs,
                                identifier: &scope.identifier(),
                            })
                            .unwrap();
                            impl_tx.run_block(&executor_name, data).await;

                            let tx_clone = tx.clone();

                            let session_id_clone = session_id.clone();
                            _ = tokio::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                                let data = serde_json::to_vec(&ReceiveMessage::ListenerTimeout {
                                    job_id: job_id.clone(),
                                    session_id: session_id_clone.clone(),
                                })
                                .unwrap();

                                tx_clone
                                    .send(SchedulerCommand::ReceiveMessage(data))
                                    .unwrap();
                            })
                        }
                    }
                    Ok(SchedulerCommand::SpawnExecutorTimeout {
                        executor,
                        package,
                        identifier,
                    }) => {
                        for (_, sender) in subscribers.iter() {
                            sender
                                .send(ReceiveMessage::ExecutorTimeout {
                                    session_id: session_id.clone(),
                                    executor_name: executor.clone(),
                                    package: package.clone(),
                                    identifier: identifier.clone(),
                                })
                                .unwrap();
                        }
                    }
                    Ok(SchedulerCommand::ExecutorExit {
                        executor,
                        code,
                        reason,
                    }) => {
                        // 理论上有任意一个 block 处理就 OK
                        for (_, sender) in subscribers.iter() {
                            sender
                                .send(ReceiveMessage::ExecutorExit {
                                    session_id: session_id.clone(),
                                    executor_name: executor.clone(),
                                    code,
                                    reason: reason.clone(),
                                })
                                .unwrap();
                        }
                    }
                    Ok(SchedulerCommand::ReceiveMessage(data)) => {
                        if let Some(msg) = parse_worker_message(data, &session_id) {
                            tracing::info!("Receive message: {:?}", msg);
                            match msg {
                                ReceiveMessage::ExecutorReady {
                                    executor_name,
                                    package,
                                    session_id,
                                    identifier,
                                } => {
                                    // same as generate_executor_map_name fn logic
                                    let executor_map_name = if let Some(ref id) = identifier {
                                        format!("{}-{}", executor_name, id)
                                    } else {
                                        executor_name.clone()
                                    };

                                    let pid = {
                                        let read_map = executor_map.read().unwrap();
                                        read_map
                                            .get(&executor_map_name)
                                            .cloned()
                                            .unwrap_or_default()
                                            .pid
                                    };

                                    let mut write_map = executor_map.write().unwrap();
                                    write_map.insert(
                                        executor_map_name,
                                        ExecutorState {
                                            spawn_state: ExecutorSpawnState::Ready,
                                            pid,
                                        },
                                    );

                                    // iterator all subscribers and send executor ready message
                                    for (_, sender) in subscribers.iter() {
                                        sender
                                            .send(ReceiveMessage::ExecutorReady {
                                                executor_name: executor_name.clone(),
                                                package: package.clone(),
                                                session_id: session_id.clone(),
                                                identifier: identifier.clone(),
                                            })
                                            .unwrap();
                                    }
                                }
                                ReceiveMessage::BlockRequest(request) => {
                                    // Handle block request
                                    if let Some(sender) = subscribers.get(request.job_id()) {
                                        sender.send(ReceiveMessage::BlockRequest(request)).unwrap();
                                    } else {
                                        warn!("No subscriber for job_id: {:?}", request.job_id());
                                    }
                                }
                                _ => {
                                    if let Some(sender) =
                                        msg.job_id().and_then(|f| subscribers.get(f))
                                    {
                                        sender.send(msg).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    Ok(SchedulerCommand::Abort) => {
                        {
                            // TODO: global service will be kill as well, try to find a better way to handle this
                            //      maybe we can differentiate normal exit and signal exit
                            let read_map = executor_map.read().unwrap();
                            for (executor_name, state) in read_map.iter() {
                                info!("kill executor: {:?}", state);
                                if state.spawn_state != ExecutorSpawnState::None {
                                    if let Some(pid) = state.pid {
                                        info!("kill executor: {} pid: {}", executor_name, pid);
                                        let output = process::Command::new("kill")
                                            .arg(pid.to_string())
                                            .output();
                                        info!("kill executor output: {:?}", output);
                                    }
                                }
                            }
                        }
                        impl_tx.disconnect().await;
                        break;
                    }
                    Err(e) => {
                        error!("Scheduler event-loop breaks unexpectedly: {:?}", e);
                        break;
                    }
                }
            }
        })
    }
}

fn parse_worker_message(data: MessageData, session_id: &SessionId) -> Option<ReceiveMessage> {
    match serde_json::from_slice::<ReceiveMessage>(&data) {
        Ok(msg) => {
            if msg.session_id() == session_id {
                Some(msg)
            } else {
                None
            }
        }
        Err(e) => {
            let str = String::from_utf8(data).unwrap_or("deserialize error".to_string());
            warn!(
                "Incorrect message sending to scheduler. session_id: {:?} error: {:?} data:{:?}",
                session_id, e, str
            );
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutorParameters {
    pub addr: String,
    pub session_id: SessionId,
    pub session_dir: String,
    pub pass_through_env_keys: Vec<String>,
    pub bind_paths: Vec<BindPath>,
    pub env_file: Option<String>,
    pub tmp_dir: PathBuf,
    pub debug: bool,
    pub wait_for_client: bool,
}

pub fn create<TT, TR>(
    impl_tx: TT,
    impl_rx: TR,
    default_package: Option<String>,
    exclude_packages: Option<Vec<String>>,
    executor_payload: ExecutorParameters,
    data_dir: String,
) -> (SchedulerTx, SchedulerRx<TT, TR>)
where
    TT: SchedulerTxImpl,
    TR: SchedulerRxImpl,
{
    let (tx, rx) = flume::unbounded();
    (
        SchedulerTx {
            tx: tx.clone(),
            default_package,
            exclude_packages,
            data_dir,
        },
        SchedulerRx {
            impl_tx,
            impl_rx,
            executor_map: default::Default::default(),
            executor_payload,
            tx,
            rx,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_options() {
        let raw_str = r#"{"target": {"to_node": [{"node_id": "node1","input_handle": "input1"}]}}"#;

        let op: OutputOptions = serde_json::from_str(raw_str).unwrap();
        assert!(op.target.map_or(false, |t| {
            if t.to_node.is_some_and(|mut inputs| {
                inputs.pop().is_some_and(|node_input| {
                    node_input.eq(&ToNodeInput {
                        node_id: format!("node1").into(),
                        input_handle: format!("input1").into(),
                    })
                })
            }) {
                true
            } else {
                false
            }
        }));
    }
}
