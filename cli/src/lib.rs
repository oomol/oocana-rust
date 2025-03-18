mod layer;
mod cache;
mod query;
mod parser;
mod fun;

use fun::bind_path_file;
use std::{collections::HashSet, io::BufRead};
use cache::CacheAction;
use ::layer::BindPath;
use one_shot::one_shot::{run_block, BlockArgs};

use clap::{Parser, Subcommand};
use tracing::{debug, warn};
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
        #[arg(help = "Paths to search for blocks. Fallback to the directory of current flow block.", long)]
        block_search_paths: Option<String>,
        #[arg(help = "id to mark this execution session. If not provided, a UUID will be randomly generated different value as the default value for that run.", long, default_value_t = Uuid::new_v4().to_string())]
        session: String,
        #[arg(help = "Enable reporter.", long)]
        reporter: bool,
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
        #[arg(help = "temporary session path,  It will be the return value of context.sessionDir or context.session_dir function. just a string, oocana just check if it is exist.", long)]
        session_dir: Option<String>,
        #[arg(help = "extra bind paths, format <source_path>:<target_path>, accept multiple input. example: --bind-paths <source>:<target> --bind-paths <source>:<target>", long)]
        extra_bind_paths: Option<Vec<String>>,
        #[arg(help = "when spawn a new process, retain the environment variables(only accept variable name), accept multiple input. example: --retain-env-keys <env> --retain-env-keys <env>", long)]
        retain_env_keys: Option<Vec<String>>,
        #[arg(help = "env files, only support json file for now, accept multiple input. example: --env-files <file> --env-files <file>. all env file will be processed. first root key will be env's key, the value will be pass through. Only available for python and nodejs executor.", long)]
        env_files: Option<Vec<String>>,
        #[arg(help = "bind path from file, format is <source_path>:<target_path> line by line, if not provided, it will be find env OOCANA_BIND_PATH_FILE variable", long, default_value_t = bind_path_file())]
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
        Commands::Run { session, .. } => {
            utils::logger::setup_logging(LogParams {
                sub_dir: Some(format!("sessions/{session}")),
                log_name: "oocana",
                output_to_console: false,
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
        Commands::Run { block, broker, block_search_paths, session, reporter, use_cache, nodes, input_values, exclude_packages, default_package, extra_bind_paths, session_dir: session_path, retain_env_keys, env_files, bind_path_file } => {

            let bind_paths = fun::bind_path(extra_bind_paths, bind_path_file);

            run_block(BlockArgs {
                block_path: block,
                broker_address: broker.to_owned(),
                block_search_paths: block_search_paths.as_ref()
                .map(|p| p.split(',').map(|s| parser::expand_tilde(s)).collect()),
                session: session.to_owned(),
                reporter_enable: reporter.to_owned(),
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
                bind_paths: Some(bind_paths),
                retain_env_keys: retain_env_keys.to_owned(),
                env_files: env_files.to_owned(),
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
