mod layer;
mod cache;
mod query;
mod parser;
mod fun;

use fun::{bind_path_file, env_file};
use std::collections::HashSet;
use cache::CacheAction;
use one_shot::one_shot::{run_block, BlockArgs};

use clap::{Parser, Subcommand};
use tracing::debug;
use utils::{error::Result, logger::LogParams};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(
    name = "oocana",
    author,
    about,
    long_about = "Oocana CLI",
    version,
    subcommand_required = true,
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");


#[derive(Subcommand, Debug)]
enum Commands {
    #[command(
        name = "run",
        about = "Run a Oocana Flow",
        long_about = None, 
    )]
    Run {
        #[arg(help = "Absolute Path to the Oocana Block Manifest file or a directory with flow.oo.yaml.")]
        block: String,
        #[arg(help = "message report Address. format is ip:port", long)]
        broker: Option<String>,
        #[arg(help = "Paths to search for Packages. Fallback to the directory of current flow block.", long)]
        search_paths: Option<String>,
        #[arg(help = "id to mark this execution session. If not provided, a UUID will be randomly generated different value as the default value for that run.", long, default_value_t = Uuid::new_v4().to_string())]
        session: String,
        #[arg(help = "Enable reporter.", long)]
        reporter: bool,
        #[arg(help = "Verbose output. If true oocana will print all log message to console output", long)]
        verbose: bool,
        #[arg(help = "Debug mode. If enable, when oocana spawn executor it will give some debugging message to every executor to make they support debugging. Only support in python-executor and nodejs-executor now", long)]
        debug: bool,
        #[arg(help = "Wait for client to connect. If true, when oocana spawn executor, the executor will wait for client to connect before start the flow. Only support in python-executor and nodejs-executor now", long)]
        wait_for_client: bool,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
        #[arg(help = "Stop the flow after the node is finished.", long)]
        nodes: Option<String>,
        #[arg(help = "Values for the input handles value. format is {\"node_id\": \"inputHandleName\": [1]}}. first key is node id, the first level value is a key-value pair, the next level's value is a list of input values", long)]
        input_values: Option<String>,
        #[arg(help = "default package environment, any block has no package will use this package environment", long)]
        default_package: Option<String>,
        #[arg(help = "exclude package, accept package path", long)]
        exclude_packages: Option<String>,
        #[arg(help = "a directory which will pass to every block, oocana just check the if the path is exit, if not oocana will create one. Oocana won't do anything about this path, won't delete it. It will be the return value of context.sessionDir or context.session_dir function.", long)]
        session_dir: Option<String>,
        #[arg(help = "a temporary root directory. oocana will create a sub directory (calculate with the block path hash) in the root directory. The sub directory path will be context.tempDir or context.temp_dir function's return value. This sub directory will be deleted if this session success and will retain if session failed. If not provided, oocana will search OOCANA_TEMP_ROOT. If still no value the temp_root will be use os's temp dir.", long, default_value_t = fun::temp_root())]
        temp_root: String,
        #[arg(help = "when spawn a new process, retain the environment variables(only accept variable name), accept multiple input. example: --retain-env-keys <env> --retain-env-keys <env>", long)]
        retain_env_keys: Option<Vec<String>>,
        #[arg(help = ".env file path, when spawn a executor, these env will pass to this executor. The file format is <key>=<value> line by line like traditional env file. if not provided, oocana will search OOCANA_ENV_FILE env variable", long, default_value_t = env_file())]
        env_file: String,
        #[arg(help = "bind paths, format <source_path>:<target_path>, accept multiple input. example: --bind-paths <source>:<target> --bind-paths <source>:<target>", long)]
        bind_paths: Option<Vec<String>>,
        #[arg(help = "a file path contains multiple bind paths. The file format is <source_path>:<target_path> line by line, if not provided, it will be found in OOCANA_BIND_PATH_FILE env variable", long, default_value_t = bind_path_file())]
        bind_path_file: String,
    },
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    Query {
        #[command(subcommand)]
        action: query::QueryAction,
    },
    #[command(
        name = "package-layer",
        about = "Package Layer action api",
        long_about = None, 
    )]
    PackageLayer {
        #[command(subcommand)]
        action: layer::LayerAction,
    }
}

pub fn cli_match() -> Result<()> {
    let cli = Cli::parse();

    let command = &cli.command;

    let _guard = match command {
        Commands::Run { session, verbose, .. } => {
            utils::logger::setup_logging(LogParams {
                sub_dir: Some(format!("sessions/{session}")),
                log_name: "oocana",
                output_to_console: *verbose,
                capture_stdout_stderr_target: false,
            })?
        },
        Commands::PackageLayer { action } => {
            utils::logger::setup_logging({
                LogParams {
                    sub_dir: Some("package-layer"),
                    log_name: "action",
                    // create 要将特定 stdout stderr 的 target 输出到控制台。因此两个都要为 true。
                    output_to_console: match action {
                        layer::LayerAction::Create { .. } => true,
                        _ => false,
                    },
                    capture_stdout_stderr_target: match action {
                        layer::LayerAction::Create { .. } => true,
                        _ => false,
                    }
                }
            })?
        },
        Commands::Query { action } => {
            utils::logger::setup_logging({
                LogParams {
                    sub_dir: Some("query"),
                    log_name: match action {
                        query::QueryAction::Upstream {  .. } => "upstream",
                        query::QueryAction::Service { .. } => "service",
                        query::QueryAction::Package { .. } => "package",
                    },
                    output_to_console: false,
                    capture_stdout_stderr_target: false,
                }
            })?
        },
        Commands::Cache { .. } => {
            utils::logger::setup_logging({
                LogParams {
                    sub_dir: Some("cache"),
                    log_name: "action",
                    output_to_console: false,
                    capture_stdout_stderr_target: false,
                }
            })?
        }
    };

    debug!("run cli args: {command:#?} in version: {VERSION}");
    match command {
        Commands::Run { block, broker, search_paths, session, reporter, debug, wait_for_client, use_cache, nodes, input_values, exclude_packages, default_package, bind_paths, session_dir: session_path, retain_env_keys, env_file, bind_path_file, verbose: _verbose, temp_root } => {

            let bind_paths = fun::bind_path(bind_paths, bind_path_file);
            let envs = fun::envs(&env_file);

            run_block(BlockArgs {
                block_path: block,
                broker_address: broker.to_owned(),
                search_paths: search_paths.as_ref()
                .map(|p| p.split(',').map(|s| parser::expand_tilde(s)).collect()),
                session: session.to_owned(),
                reporter_enable: reporter.to_owned(),
                debug: debug.to_owned(),
                wait_for_client: wait_for_client.to_owned(),
                use_cache: use_cache.to_owned(),
                nodes: nodes.as_ref().map(|nodes| {
                    nodes
                        .split(',')
                        .map(|node| node.to_string())
                        .collect::<HashSet<String>>()
                }),
                input_values: input_values.to_owned(),
                default_package: default_package.to_owned(),
                exclude_packages: exclude_packages.as_ref()
                .map(|p| p.split(',').map(|s| s.to_string()).collect()),
                session_dir: session_path.to_owned(),
                bind_paths: bind_paths,
                retain_env_keys: retain_env_keys.to_owned(),
                envs,
                temp_root: temp_root.to_owned(),
            })?
        },
        Commands::Cache { action } => {
            cache::cache_action(action)?;
        },
        Commands::Query { action } => {
            query::query(action)?;
        },
        Commands::PackageLayer { action} => {
            layer::layer_action(action)?;
        },
    }

    Ok(())
}
