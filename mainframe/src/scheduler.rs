use async_trait::async_trait;
use flume::{Receiver, Sender};
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

use layer::{create_runtime_layer, BindPath, InjectionParams, RuntimeLayer};

use job::{BlockInputs, BlockJobStackLevel, JobId, SessionId};

use manifest_meta::{
    HandleName, InjectionStore, InputDefPatchMap, InputHandles, JsonValue, OutputHandles,
    RunningScope, ServiceExecutorOptions, TaskBlockExecutor,
};
use tokio::io::AsyncBufReadExt;

use tokio::process::Command as tokioCommand;
use tracing::{debug, error, info, instrument, warn};

use utils::error::{Error, Result};

use crate::MessageData;

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
        done: bool,
        handle: HandleName,
        output: JsonValue,
    },
    BlockError {
        session_id: SessionId,
        job_id: JobId,
        error: String,
    },
    BlockFinished {
        session_id: SessionId,
        job_id: JobId,
        error: Option<String>,
    },
    ExecutorReady {
        session_id: SessionId,
        executor_name: String,
        package: Option<String>,
        identifier: Option<String>,
    },
    // --- 以下消息，是通过 scheduler 发送给 subscriber 的消息，而不是 mqtt 消息 --- //
    ExecutorTimeout {
        session_id: SessionId,
        executor_name: String,
        package: Option<String>,
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
            ReceiveMessage::BlockError { session_id, .. } => session_id,
            ReceiveMessage::BlockFinished { session_id, .. } => session_id,
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
            ReceiveMessage::BlockError { job_id, .. } => Some(job_id),
            ReceiveMessage::BlockFinished { job_id, .. } => Some(job_id),
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
        package: &'a Option<String>,
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
        package: &'a Option<String>,
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
    async fn send_inputs(&self, job_id: &JobId, data: MessageData);
    async fn run_block(&self, executor_name: &String, data: MessageData);
    async fn run_service_block(&self, executor_name: &String, data: MessageData);
    async fn disconnect(&self);
}

#[async_trait]
pub trait SchedulerRxImpl {
    async fn recv(&mut self) -> MessageData;
}

enum SchedulerCommand {
    RegisterSubscriber(JobId, Sender<ReceiveMessage>),
    UnregisterSubscriber(JobId),
    SendInputs {
        job_id: JobId,
        stacks: Vec<BlockJobStackLevel>,
        block_path: Option<String>,
        inputs: Option<BlockInputs>,
        inputs_def: Option<InputHandles>,
        inputs_def_patch: Option<InputDefPatchMap>,
    },
    ExecuteBlock {
        job_id: JobId,
        executor_name: String,
        dir: String,
        stacks: Vec<BlockJobStackLevel>,
        outputs: Option<OutputHandles>,
        executor: TaskBlockExecutor,
        injection_store: Option<InjectionStore>,
        scope: RunningScope,
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
        scope: RunningScope,
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
    Spawned, // spawn 但是没有收到 ready 信息
    Ready,   // 收到 ready 信息
}

#[derive(Debug, Clone)]
pub struct SchedulerTx {
    tx: Sender<SchedulerCommand>,
    default_package: Option<String>,
    exclude_packages: Option<Vec<String>>,
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
    pub scope: &'a RunningScope,
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
    pub scope: &'a RunningScope,
    pub flow: &'a Option<String>,
}

pub struct ExecutorCheckResult {
    pub executor_state: ExecutorSpawnState,
    pub executor_map_name: String,
    pub identifier: Option<String>,
    pub layer: Option<RuntimeLayer>, // layer is only exist when executor_exist is false
}

pub struct ExecutorCheckParams<'a> {
    pub executor_name: &'a str,
    pub scope: &'a RunningScope,
    pub injection_store: &'a Option<InjectionStore>,
    pub flow: &'a Option<String>,
    pub executor_payload: &'a ExecutorPayload,
    pub executor_map: Arc<RwLock<HashMap<String, ExecutorState>>>,
}

fn generate_executor_map_name(executor_name: &str, scope: &RunningScope) -> String {
    if let Some(id) = scope.identifier() {
        format!("{}-{}", executor_name, id)
    } else {
        executor_name.to_owned()
    }
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
                stacks: stacks,
                block_path,
                inputs: inputs,
                inputs_def,
                inputs_def_patch: inputs_def_patch,
            })
            .unwrap();
    }

    /** TODO: generate default scope instead of default package. */
    /** filter some scope, move then to default scope */
    pub fn calculate_scope(&self, scope: &RunningScope) -> RunningScope {
        match self.exclude_packages.as_ref() {
            Some(exclude_packages) => {
                if let Some(pkg) = scope.package_path() {
                    let pkg_str = pkg.to_string_lossy().to_string();
                    if exclude_packages.contains(&pkg_str) {
                        match self.default_package {
                            Some(ref default_package) => RunningScope::Package {
                                path: PathBuf::from(default_package.clone()),
                                name: Some("default".to_string()),
                                node_id: None,
                            },
                            None => RunningScope::Global { node_id: None },
                        }
                    } else {
                        return scope.clone();
                    }
                } else {
                    return scope.clone();
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
                dir: dir.to_owned(),
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
    executor_payload: ExecutorPayload,
    tx: Sender<SchedulerCommand>,
    rx: Receiver<SchedulerCommand>,
}

fn spawn_executor(
    executor: &str,
    layer: Option<RuntimeLayer>,
    scope: &RunningScope,
    executor_map: Arc<RwLock<HashMap<String, ExecutorState>>>,
    executor_payload: ExecutorPayload,
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

    let ExecutorPayload {
        session_id,
        addr,
        session_dir,
        pass_through_env_keys,
        bind_paths: _bind_paths,
        envs: executor_envs,
    } = &executor_payload;

    // 后面加 -executor 尾缀是一种隐式约定。例如：如果 executor 是 "python"，那么实际上会执行 python-executor。
    // 目前约定 executor 执行文件在 PATH 环境变量中。
    let executor_bin = executor.to_owned() + "-executor";

    let mut log_filename = None;

    let mut executor_package: Option<String> = None;
    let mut spawn_suffix = None;

    let identifier = scope.identifier().unwrap_or_default();
    let scope_package = scope
        .package_path()
        .map(|f| f.to_string_lossy().to_string());

    let mut command = if let Some(ref pkg_layer) = layer {
        let package_path_str = pkg_layer.package_path.to_string_lossy();

        let hash = calculate_short_hash(&package_path_str, 8);
        spawn_suffix = Some(hash.clone());

        let mut exec_form_cmd: Vec<&str> = vec![
            &executor_bin,
            "--session-id",
            session_id,
            "--address",
            addr,
            "--session-dir",
            session_dir,
            "--suffix",
            &hash,
        ];

        if identifier.len() > 0 {
            exec_form_cmd.push("--identifier");
            exec_form_cmd.push(&identifier);
        }

        if scope_package.is_some() {
            exec_form_cmd.push("--package");
            exec_form_cmd.push(&scope_package.as_ref().unwrap());
        }

        executor_package = Some(package_path_str.to_string());

        log_filename = Some(format!("ovmlayer-{}-{}", executor_bin.to_owned(), hash));

        let script_str = layer::convert_to_script(&exec_form_cmd);
        let cmd = pkg_layer.run_command(&script_str);

        cmd
    } else {
        let mut args = vec![
            "--session-id",
            session_id,
            "--address",
            addr,
            "--session-dir",
            session_dir,
        ];

        if identifier.len() > 0 {
            args.push("--identifier");
            args.push(&identifier);
        }

        if scope_package.is_some() {
            args.push("--package");
            args.push(&scope_package.as_ref().unwrap());
        }

        let mut cmd = process::Command::new(executor_bin.to_owned());
        cmd.args(args);
        cmd
    };

    let mut envs: HashMap<String, String> = std::env::vars()
        .filter(|(key, _)| key.starts_with("OOMOL_") || pass_through_env_keys.contains(key))
        .collect();

    if let Some(log_file) = log_filename {
        let log_dir = utils::logger::logger_dir();
        envs.insert(
            layer::OVMLAYER_LOG_ENV_KEY.to_owned(),
            log_dir.join(log_file).to_string_lossy().to_string(),
        );
    }

    tracing::debug!("pass through these env keys: {:?}", envs.keys());

    for (key, value) in executor_envs.iter() {
        if envs.contains_key(key) {
            warn!("env key {} is already in envs, skip", key);
        } else {
            envs.insert(key.to_owned(), value.to_owned());
        }
    }

    command
        .env("IS_FORKED", "1")
        .envs(envs)
        .stdin(process::Stdio::null())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    info!("spawn executor: {:?}", command);
    let mut tokio_command = tokioCommand::from(command);

    let child = tokio_command.spawn();

    let tx = tx.clone();

    let executor_map_clone = executor_map.clone();
    let txx = tx.clone();

    let executor_bin_clone = executor_bin.clone();
    let executor_map_name_clone = executor_map_name.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        {
            let read_map = executor_map_clone.read().unwrap();
            let executor_state = read_map
                .get(&executor_map_name_clone)
                .cloned()
                .unwrap_or_default();
            if executor_state.spawn_state == ExecutorSpawnState::Ready {
                return;
            }
            txx.send(SchedulerCommand::SpawnExecutorTimeout {
                executor: executor_bin_clone,
                package: executor_package,
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
                    pid: pid.clone(),
                },
            );
            drop(map);

            let spawn_suffix = spawn_suffix;
            if let Some(stdout) = ch.stdout.take() {
                let mut reader = tokio::io::BufReader::new(stdout).lines();
                let executor_bin_clone = executor_bin.clone();
                let spawn_suffix_clone = spawn_suffix.clone();
                tokio::spawn(async move {
                    while let Ok(Some(line)) = reader.next_line().await {
                        debug!(
                            "{}{} stdout: {}",
                            executor_bin_clone,
                            spawn_suffix_clone
                                .as_ref()
                                .map_or_else(|| "".to_string(), |suffix| format!("-{}", suffix)),
                            line
                        );
                    }
                });
            }

            if let Some(stderr) = ch.stderr.take() {
                let mut reader = tokio::io::BufReader::new(stderr).lines();
                let executor_bin_clone = executor_bin.clone();
                let spawn_suffix_clone = spawn_suffix.clone();
                tokio::spawn(async move {
                    while let Ok(Some(line)) = reader.next_line().await {
                        error!(
                            "{}{} stderr: {}",
                            executor_bin_clone,
                            spawn_suffix_clone
                                .as_ref()
                                .map_or_else(|| "".to_string(), |suffix| format!("-{}", suffix)),
                            line
                        );
                    }
                });
            }
            let executor_bin_clone = executor_bin.clone();
            let executor_map_clone = executor_map.clone();
            let executor_map_name_clone = executor_map_name.clone();

            tokio::spawn(async move {
                let status = ch.wait().await;
                let mut write_map = executor_map_clone.write().unwrap();
                write_map.insert(
                    executor_map_name_clone.clone(),
                    ExecutorState {
                        spawn_state: ExecutorSpawnState::None,
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
            return Result::Ok(());
        }
        Err(e) => {
            let mut write_map = executor_map.write().unwrap();
            write_map.insert(
                executor_map_name.clone(),
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
            return Result::Err(Error::new(&message));
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
    let no_layer_feature = !layer::feature_enabled();
    let executor_map_name = generate_executor_map_name(executor_name, &scope);

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
            executor_map_name,
            identifier: scope.identifier(),
            layer: None,
        });
    } else if no_layer_feature {
        return Ok(ExecutorCheckResult {
            executor_state,
            executor_map_name,
            identifier: scope.identifier(),
            layer: None,
        });
    }

    let layer = if let Some(pkg) = scope.package_path() {
        let mut bind_paths = executor_payload.bind_paths.clone();

        if let Some(store) = injection_store {
            if let Some(target) = scope.target() {
                if let Some(meta) = store.get(&target) {
                    for node in meta.nodes.iter() {
                        bind_paths.push(BindPath {
                            source: node
                                .absolute_entry
                                .parent()
                                .map(|p| p.to_string_lossy().to_string())
                                .unwrap_or_default(),
                            target: format!(
                                "{}/{}",
                                pkg.to_string_lossy().to_string(),
                                node.relative_entry
                                    .parent()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_default()
                            ),
                        });
                    }
                }
            }
        }
        let path_str = pkg.to_string_lossy().to_string();
        let mut runtime_layer =
            create_runtime_layer(&path_str, &bind_paths, &executor_payload.envs)?;

        if let Some(store) = injection_store {
            if let Some(target) = scope.target() {
                if let Some(meta) = store.get(&target) {
                    let scripts = meta.scripts.clone().unwrap_or_default();

                    let result = runtime_layer.inject_runtime_layer(InjectionParams {
                        package_version: &meta.package_version,
                        package_path: &path_str,
                        scripts: &scripts,
                        flow: &flow.as_ref().unwrap_or(&"".to_string()),
                    });

                    if let Err(e) = result {
                        return Result::Err(e);
                    }
                }
            }
        }

        Some(runtime_layer)
    } else {
        info!("final package is None, skip layer creation {:?}", scope);
        None
    };

    Ok(ExecutorCheckResult {
        executor_state,
        executor_map_name,
        identifier: scope.identifier(),
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
                        debug_assert!(subscribers.get(&job_id).is_none());
                        subscribers.insert(job_id, sender);
                    }
                    Ok(SchedulerCommand::UnregisterSubscriber(job_id)) => {
                        subscribers.remove(&job_id);
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
                            return;
                        }

                        let ExecutorCheckResult {
                            executor_state,
                            executor_map_name,
                            identifier,
                            layer,
                        } = result.unwrap();

                        info!(
                            "execute service block. executor: {:?} executor_map: {} state: {:?}",
                            executor_name, executor_map_name, executor_state
                        );
                        if executor_state == ExecutorSpawnState::None {
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
                            let data = serde_json::to_vec(&ExecutePayload::ServiceBlockPayload {
                                session_id: &session_id,
                                job_id: &job_id,
                                stacks: &stacks,
                                executor_name: &executor_name,
                                dir: &dir,
                                block_name: &block_name,
                                service_executor: &service_executor,
                                outputs: &outputs,
                                service_hash: service_hash,
                                package: &identifier,
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
                            tx.send(SchedulerCommand::ExecutorExit {
                                executor: executor_name.clone(),
                                code: -1,
                                reason: Some(format!("{}", e)),
                            })
                            .unwrap();
                            return;
                        }

                        let ExecutorCheckResult {
                            executor_state,
                            executor_map_name,
                            identifier,
                            layer,
                        } = result.unwrap();

                        info!(
                            "execute block. executor: {:?} executor_map: {} state: {:?}",
                            executor_name, executor_map_name, executor_state
                        );
                        if executor_state == ExecutorSpawnState::None {
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
                            let data = serde_json::to_vec(&ExecutePayload::BlockPayload {
                                session_id: &session_id,
                                executor_name: &executor_name,
                                job_id: &job_id,
                                stacks: &stacks,
                                dir: &dir,
                                executor: &executor,
                                outputs: &outputs,
                                package: &identifier,
                            })
                            .unwrap();
                            impl_tx.run_block(&executor_name, data).await;

                            let tx_clone = tx.clone();

                            let session_id_clone = session_id.clone();
                            _ = tokio::spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
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
                    Ok(SchedulerCommand::SpawnExecutorTimeout { executor, package }) => {
                        for (_, sender) in subscribers.iter() {
                            sender
                                .send(ReceiveMessage::ExecutorTimeout {
                                    session_id: session_id.clone(),
                                    executor_name: executor.clone(),
                                    package: package.clone(),
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
                                    package: identifier,
                                    session_id,
                                } => {
                                    // this logic is shoud keep the same with generate_executor_map_name bind
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
                                            pid: pid,
                                        },
                                    );

                                    // iterator all subscribers and send executor ready message
                                    for (_, sender) in subscribers.iter() {
                                        sender
                                            .send(ReceiveMessage::ExecutorReady {
                                                executor_name: executor_name.clone(),
                                                package: identifier.clone(),
                                                session_id: session_id.clone(),
                                            })
                                            .unwrap();
                                    }
                                }
                                _ => {
                                    if let Some(sender) =
                                        msg.job_id().and_then(|f| subscribers.get(&f))
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
pub struct ExecutorPayload {
    addr: String,
    session_id: SessionId,
    session_dir: String,
    pass_through_env_keys: Vec<String>,
    bind_paths: Vec<BindPath>,
    envs: HashMap<String, String>,
}

pub fn create<TT, TR>(
    session_id: SessionId,
    addr: String,
    bind_paths: Vec<BindPath>,
    impl_tx: TT,
    impl_rx: TR,
    default_package: Option<String>,
    exclude_packages: Option<Vec<String>>,
    session_dir: String,
    retain_env_keys: Vec<String>,
    envs: HashMap<String, String>,
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
        },
        SchedulerRx {
            impl_tx,
            impl_rx,
            executor_map: default::Default::default(),
            executor_payload: ExecutorPayload {
                addr: addr,
                session_id: session_id,
                session_dir: session_dir,
                pass_through_env_keys: retain_env_keys,
                bind_paths: bind_paths,
                envs,
            },
            tx,
            rx,
        },
    )
}
