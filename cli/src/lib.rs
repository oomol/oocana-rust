mod layer;
mod cache;
mod query;
mod fun;

use std::{collections::HashSet, path::PathBuf};
use cache::CacheAction;
use fun::arg::{find_env_file, load_bind_paths, parse_search_paths, temp_root, config};
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
    #[arg(help = "oocana configuration file path, if not provided, will search OOCANA_CONFIG, if still not found, defaults to '~/.oocana/config'", long, default_value_t = config())]
    config: String,
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
        #[arg(help = "message report Address. format is ip:port. default is 127.0.0.1:47688", long)]
        broker: Option<String>,
        #[arg(help = "Paths to search for Packages. Fallback to the directory of current flow block.", long, alias = "block-search-paths")]
        search_paths: Option<String>,
        #[arg(help = "id to mark this execution session. If not provided, a UUID will be randomly generated different value as the default value for that run.", long, default_value_t = Uuid::new_v4().to_string())]
        session: String,
        #[arg(help = "Enable reporter.", long, num_args =0..=1, require_equals=true, default_missing_value = "true")]
        reporter: Option<bool>,
        #[arg(help = "Verbose output. If true oocana will print all log message to console output", long)]
        verbose: bool,
        #[arg(help = "Debug mode. If enable, when oocana spawn executor it will give some debugging message to every executor to make they support debugging. Only support in python-executor and nodejs-executor now", long, num_args =0..=1, require_equals=true, default_missing_value = "true")]
        debug: Option<bool>,
        #[arg(help = "Wait for client to connect. If true, when oocana spawn executor, the executor will wait for client to connect before start the flow. Only support in python-executor and nodejs-executor now", long)]
        wait_for_client: bool,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
        #[arg(help = "Stop the flow after the node is finished.", long)]
        nodes: Option<String>,
        #[arg(help = "Values for the input handles value. It's used to fulfill a block's inputs definition. Format is {\"inputHandleName\": <VALUE>} where the first key is the handle name, and the first-level value is a key-value pair.", long)]
        inputs: Option<String>,
        #[arg(help = "Values for the flow nodes' input handle value. It's used when a block has flow node inputs. Format is {\"node_id\": {\"inputHandleName\": <VALUE>}}. First key is node id, the first level value is a key-value pair, and the next level's value is input values", long)]
        nodes_inputs: Option<String>,
        #[arg(help = "default package environment, any block has no package will use this package environment", long)]
        default_package: Option<String>,
        #[arg(help = "exclude package, these package will skip layer feature, accept package path", long)]
        exclude_packages: Option<String>,
        #[arg(help = "a directory which will pass to every block, oocana just check the if the path is exit, if not oocana will create one. Oocana won't do anything about this path, won't delete it. It will be the return value of context.sessionDir or context.session_dir function.", long)]
        session_dir: Option<String>,
        #[arg(help = "A directory that can be used for persistent data storage. Flows and blocks that are not part of a package will use this directory.", long, default_value_t = temp_root())]
        project_data: String,
        #[arg(help = "a directory that can be used for persistent package data, all package's data will store in this directory. it can persist across sessions", long, default_value_t = temp_root())]
        pkg_data_root: String,
        #[arg(help = "a temporary root directory. oocana will create a sub directory (calculate with the block path hash) in the root directory. The sub directory path will be context.tempDir or context.temp_dir function's return value. This sub directory will be deleted if this session success and will retain if session failed. If not provided, oocana will search OOCANA_TEMP_ROOT. If still no value the temp_root will be use os's temp dir.", long, default_value_t = temp_root())]
        temp_root: String,
        #[arg(help = "when spawn a new process, retain the environment variables(only accept variable name), accept multiple input. example: --retain-env-keys <env> --retain-env-keys <env>", long)]
        retain_env_keys: Option<Vec<String>>,
        #[arg(help = ".env file path, when spawn a executor, these env will pass to this executor. The file format is <key>=<value> line by line like traditional env file. if not provided, oocana will search OOCANA_ENV_FILE env variable", long)]
        env_file: Option<String>,
        #[arg(help = "bind paths, format src=<source_path>,dst=<target_path>,[ro|rw],[recursive|nonrecursive] (rw,nonrecursive is default value), accept multiple input. example: --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive", long)]
        bind_paths: Option<Vec<String>>,
        #[arg(help = "a file path contains multiple bind paths. The file format is src=<source_path>,dst=<target_path>,[ro|rw],[recursive|nonrecursive] (rw,nonrecursive is default value) line by line, if not provided, it will be found in OOCANA_BIND_PATH_FILE env variable", long)]
        bind_path_file: Option<String>,
        #[arg(help = "dry run, if true, oocana will not execute the flow, just print all parsed parameters", long)]
        dry_run: bool,
        #[arg(help = "If true, oocana will forward report messages to console", long)]
        report_to_console: bool,
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
        Commands::Run { session, verbose, report_to_console, .. } => {
            utils::logger::setup_logging(LogParams {
                sub_dir: Some(format!("sessions/{session}")),
                log_name: "oocana",
                output_to_console: *verbose,
                capture_stdout_stderr_target: *report_to_console,
            })?
        },
        Commands::PackageLayer { action } => {
            utils::logger::setup_logging({
                LogParams {
                    sub_dir: Some("package-layer"),
                    log_name: "action",
                    // create 要将特定 stdout stderr 的 target 输出到控制台。因此两个都要为 true。
                    output_to_console: matches!(action, layer::LayerAction::Create { .. }),
                    capture_stdout_stderr_target: matches!(action, layer::LayerAction::Create { .. }),
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
                        query::QueryAction::NodesInputs { .. } => "nodes-inputs",
                        query::QueryAction::Inputs { .. } => "inputs",
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

    let app_config = utils::config::load_config(Some(&cli.config))?;
    debug!("config {:?} command args: {command:#?} in version: {VERSION}", cli.config);
    match command {
        Commands::Run { block, broker, search_paths, session, reporter, debug, wait_for_client, use_cache, nodes, nodes_inputs, inputs, exclude_packages, default_package, bind_paths, session_dir: session_path, retain_env_keys, env_file, bind_path_file, verbose: _verbose, temp_root, dry_run, pkg_data_root, project_data, report_to_console } => {

            let bind_paths = load_bind_paths(bind_paths, bind_path_file);
            let search_paths = parse_search_paths(search_paths);
            let env_file = find_env_file(env_file);

            if *dry_run {
                // print the parameters
                println!("bind_paths: {:?}", bind_paths);
                println!("search_paths: {:?}", search_paths);

                println!("dry_run is enabled, exiting without execution.");

                return  Ok(());
            } else {
                tracing::debug!("bind_paths: {:?} search_paths: {:?}", bind_paths, search_paths);
            }

            run_block(BlockArgs {
                block_path: block,
                broker_address: broker.clone().unwrap_or(app_config.run.broker),
                search_paths,
                session: session.to_owned(),
                reporter_enable: reporter.unwrap_or(app_config.run.reporter.unwrap_or_default()),
                debug: debug.unwrap_or(app_config.run.debug.unwrap_or_default()),
                wait_for_client: wait_for_client.to_owned(),
                use_cache: use_cache.to_owned(),
                nodes: nodes.as_ref().map(|nodes| {
                    nodes
                        .split(',')
                        .map(|node| node.to_string())
                        .collect::<HashSet<String>>()
                }),
                inputs: inputs.to_owned(),
                nodes_inputs: nodes_inputs.to_owned(),
                default_package: default_package.to_owned(),
                exclude_packages: exclude_packages.as_ref()
                .map(|p| p.split(',').map(|s| s.to_string()).collect())
                .or_else(|| app_config.run.exclude_packages.clone()),
                session_dir: session_path.to_owned(),
                bind_paths,
                retain_env_keys: retain_env_keys.to_owned(),
                env_file,
                temp_root: temp_root.to_owned(),
                project_data: &PathBuf::from(project_data),
                pkg_data_root: &PathBuf::from(pkg_data_root),
                report_to_console: report_to_console.to_owned(),
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
