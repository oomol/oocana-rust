mod cache;
mod fun;
mod layer;
mod query;

use cache::CacheAction;
use fun::arg::{config, find_env_file, load_bind_paths, parse_search_paths, temp_root};
use one_shot::one_shot::{BlockArgs, run_block};
use std::{collections::HashSet, path::PathBuf};

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
    subcommand_required = true
)]
pub struct Cli {
    #[arg(help = "oocana configuration file path, if not provided, will search OOCANA_CONFIG, if still not found, defaults to '~/.oocana/config'", long, default_value_t = config())]
    config: String,
    #[command(subcommand)]
    command: Commands,
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Subcommand, Debug)]
// TODO: consider boxing large fields to reduce enum size
#[allow(clippy::large_enum_variant)]
enum Commands {
    #[command(
        name = "run",
        about = "Run a Oocana Flow",
        long_about = None,
    )]
    Run {
        #[arg(
            help = "Absolute Path to the Oocana Block Manifest file or a directory with flow.oo.yaml."
        )]
        block: String,
        #[arg(
            help = "message report Address. format is ip:port. default is 127.0.0.1:47688",
            long
        )]
        broker: Option<String>,
        #[arg(
            help = "Paths to search for packages. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
        #[arg(help = "id to mark this execution session. If not provided, a UUID will be randomly generated different value as the default value for that run.", long, default_value_t = Uuid::new_v4().to_string())]
        session: String,
        #[arg(help = "Enable reporter.", long, num_args =0..=1, require_equals=true, default_missing_value = "true")]
        reporter: Option<bool>,
        #[arg(
            help = "Verbose output. If true oocana will print all log message to console output",
            long
        )]
        verbose: bool,
        #[arg(help = "Debug mode. If enable, when oocana spawn executor it will give some debugging message to every executor to make they support debugging. Only support in python-executor and nodejs-executor now", long, num_args =0..=1, require_equals=true, default_missing_value = "true")]
        debug: Option<bool>,
        #[arg(
            help = "Wait for client to connect. If true, when oocana spawn executor, the executor will wait for client to connect before start the flow. Only support in python-executor and nodejs-executor now",
            long
        )]
        wait_for_client: bool,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
        #[arg(
            help = "Stop the flow after the listed nodes are finished. Repeat the flag or use commas.",
            long,
            value_delimiter = ','
        )]
        nodes: Vec<String>,
        #[arg(
            help = "Values for the input handles value. It's used to fulfill a block's inputs definition. Format is {\"inputHandleName\": <VALUE>} where the first key is the handle name, and the first-level value is a key-value pair.",
            long
        )]
        inputs: Option<String>,
        #[arg(
            help = "Values for the flow nodes' input handle value. It's used when a block has flow node inputs. Format is {\"node_id\": {\"inputHandleName\": <VALUE>}}. First key is node id, the first level value is a key-value pair, and the next level's value is input values",
            long
        )]
        nodes_inputs: Option<String>,
        #[arg(
            help = "default package environment, any block has no package will use this package environment",
            long
        )]
        default_package: Option<String>,
        #[arg(
            help = "Exclude packages from layer mode. Repeat the flag or use commas with package paths.",
            long,
            value_delimiter = ','
        )]
        exclude_packages: Vec<String>,
        #[arg(
            help = "a directory which will pass to every block, oocana just check the if the path is exit, if not oocana will create one. Oocana won't do anything about this path, won't delete it. It will be the return value of context.sessionDir or context.session_dir function.",
            long
        )]
        session_dir: Option<String>,
        #[arg(help = "A directory that can be used for persistent data storage. Flows and blocks that are not part of a package will use this directory.", long, default_value_t = temp_root())]
        project_data: String,
        #[arg(help = "a directory that can be used for persistent package data, all package's data will store in this directory. it can persist across sessions", long, default_value_t = temp_root())]
        pkg_data_root: String,
        #[arg(help = "a temporary root directory. oocana will create a sub directory (calculate with the block path hash) in the root directory. The sub directory path will be context.tempDir or context.temp_dir function's return value. This sub directory will be deleted if this session success and will retain if session failed. If not provided, oocana will search OOCANA_TEMP_ROOT. If still no value the temp_root will be use os's temp dir.", long, default_value_t = temp_root())]
        temp_root: String,
        #[arg(
            help = "When spawning a new process, retain environment variable names. Repeat the flag or use commas.",
            long,
            value_delimiter = ','
        )]
        retain_env_keys: Vec<String>,
        #[arg(
            help = ".env file path, when spawn a executor, these env will pass to this executor. The file format is <key>=<value> line by line like traditional env file. if not provided, oocana will search OOCANA_ENV_FILE env variable",
            long
        )]
        env_file: Option<String>,
        #[arg(
            help = "bind paths, format src=<source_path>,dst=<target_path>,[ro|rw],[recursive|nonrecursive] (rw,nonrecursive is default value), accept multiple input. example: --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive --bind-paths src=<source_path>,dst=<target_path>,rw/ro,recursive/nonrecursive",
            long
        )]
        bind_paths: Option<Vec<String>>,
        #[arg(
            help = "a file path contains multiple bind paths. The file format is src=<source_path>,dst=<target_path>,[ro|rw],[recursive|nonrecursive] (rw,nonrecursive is default value) line by line, if not provided, it will be found in OOCANA_BIND_PATH_FILE env variable",
            long
        )]
        bind_path_file: Option<String>,
        #[arg(
            help = "dry run, if true, oocana will not execute the flow, just print all parsed parameters",
            long
        )]
        dry_run: bool,
        #[arg(help = "If true, oocana will forward report messages to console", long)]
        report_to_console: bool,
        #[arg(
            help = "Remote block API base URL. Overrides OOCANA_REMOTE_BLOCK_URL env var.",
            long
        )]
        remote_block_url: Option<String>,
        #[arg(
            help = "Connector API base URL. Overrides OOCANA_CONNECTOR_BASE_URL env var.",
            long
        )]
        connector_base_url: Option<String>,
        #[arg(
            help = "Timeout in seconds for remote block execution. Overrides OOCANA_REMOTE_BLOCK_TIMEOUT env var. Default is 1800 (30 minutes). Use 0 to disable timeout and poll indefinitely.",
            long
        )]
        remote_block_timeout: Option<u64>,
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
    },
}

pub fn cli_match() -> Result<()> {
    let cli = Cli::parse();

    let command = &cli.command;

    let _guard = match command {
        Commands::Run {
            session,
            verbose,
            report_to_console,
            ..
        } => utils::logger::setup_logging(LogParams {
            sub_dir: Some(format!("sessions/{session}")),
            log_name: "oocana",
            output_to_console: *verbose,
            capture_stdout_stderr_target: *report_to_console,
        })?,
        Commands::PackageLayer { action } => {
            utils::logger::setup_logging({
                LogParams {
                    sub_dir: Some("package-layer"),
                    log_name: "action",
                    // create 和 create-registry 要将特定 stdout stderr 的 target 输出到控制台。因此两个都要为 true。
                    output_to_console: matches!(
                        action,
                        layer::LayerAction::Create { .. }
                            | layer::LayerAction::CreateRegistry { .. }
                    ),
                    capture_stdout_stderr_target: matches!(
                        action,
                        layer::LayerAction::Create { .. }
                            | layer::LayerAction::CreateRegistry { .. }
                    ),
                }
            })?
        }
        Commands::Query { action } => utils::logger::setup_logging({
            LogParams {
                sub_dir: Some("query"),
                log_name: match action {
                    query::QueryAction::Upstream { .. } => "upstream",
                    query::QueryAction::Service { .. } => "service",
                    query::QueryAction::Package { .. } => "package",
                    query::QueryAction::NodesInputs { .. } => "nodes-inputs",
                    query::QueryAction::Inputs { .. } => "inputs",
                },
                output_to_console: false,
                capture_stdout_stderr_target: false,
            }
        })?,
        Commands::Cache { .. } => utils::logger::setup_logging({
            LogParams {
                sub_dir: Some("cache"),
                log_name: "action",
                output_to_console: false,
                capture_stdout_stderr_target: false,
            }
        })?,
    };

    let app_config = utils::config::load_config(Some(&cli.config))?;
    debug!(
        "config {:?} command args: {command:#?} in version: {VERSION}",
        cli.config
    );
    match command {
        Commands::Run {
            block,
            broker,
            search_paths,
            session,
            reporter,
            debug,
            wait_for_client,
            use_cache,
            nodes,
            nodes_inputs,
            inputs,
            exclude_packages,
            default_package,
            bind_paths,
            session_dir: session_path,
            retain_env_keys,
            env_file,
            bind_path_file,
            verbose: _verbose,
            temp_root,
            dry_run,
            pkg_data_root,
            project_data,
            report_to_console,
            remote_block_url,
            connector_base_url,
            remote_block_timeout,
        } => {
            let bind_paths = load_bind_paths(bind_paths, bind_path_file);
            let search_paths = parse_search_paths(search_paths);
            let env_file = find_env_file(env_file);

            if *dry_run {
                // print the parameters
                println!("bind_paths: {bind_paths:?}");
                println!("search_paths: {search_paths:?}");

                println!("dry_run is enabled, exiting without execution.");

                return Ok(());
            } else {
                tracing::debug!(
                    "bind_paths: {:?} search_paths: {:?}",
                    bind_paths,
                    search_paths
                );
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
                nodes: (!nodes.is_empty()).then(|| nodes.iter().cloned().collect::<HashSet<_>>()),
                inputs: inputs.to_owned(),
                nodes_inputs: nodes_inputs.to_owned(),
                default_package: default_package.to_owned(),
                exclude_packages: (!exclude_packages.is_empty())
                    .then_some(exclude_packages.to_owned())
                    .or_else(|| app_config.run.exclude_packages.clone()),
                session_dir: session_path.to_owned(),
                bind_paths,
                retain_env_keys: (!retain_env_keys.is_empty())
                    .then_some(retain_env_keys.to_owned()),
                env_file,
                temp_root: temp_root.to_owned(),
                project_data: &PathBuf::from(project_data),
                pkg_data_root: &PathBuf::from(pkg_data_root),
                report_to_console: report_to_console.to_owned(),
                remote_block_url: remote_block_url.to_owned(),
                connector_base_url: connector_base_url.to_owned(),
                remote_block_timeout: remote_block_timeout.to_owned(),
            })?
        }
        Commands::Cache { action } => {
            cache::cache_action(action)?;
        }
        Commands::Query { action } => {
            query::query(action)?;
        }
        Commands::PackageLayer { action } => {
            layer::layer_action(action)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse_cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("cli should parse")
    }

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn run_command_parses_all_flags() {
        let cli = parse_cli(&[
            "oocana",
            "--config",
            "/tmp/oocana.toml",
            "run",
            "tests/fixtures/connector-flow.oo.yaml",
            "--broker",
            "127.0.0.1:47688",
            "--search-paths",
            "/tmp/a,/tmp/b",
            "--session",
            "session-123",
            "--reporter=false",
            "--verbose",
            "--debug=false",
            "--wait-for-client",
            "--use-cache",
            "--nodes",
            "node-a,node-b",
            "--inputs",
            "{\"input\":1}",
            "--nodes-inputs",
            "{\"node-a\":{\"input\":1}}",
            "--default-package",
            "/pkg/default",
            "--exclude-packages",
            "/pkg/a,/pkg/b",
            "--session-dir",
            "/tmp/session",
            "--project-data",
            "/tmp/project-data",
            "--pkg-data-root",
            "/tmp/pkg-data",
            "--temp-root",
            "/tmp/temp-root",
            "--retain-env-keys",
            "FOO,BAR",
            "--env-file",
            ".env.test",
            "--bind-paths",
            "src=/src,dst=/dst,ro,recursive",
            "--bind-path-file",
            "/tmp/binds.txt",
            "--dry-run",
            "--report-to-console",
            "--remote-block-url",
            "https://remote.example",
            "--connector-base-url",
            "https://connector.example",
            "--remote-block-timeout",
            "42",
        ]);

        match cli.command {
            Commands::Run {
                block,
                broker,
                search_paths,
                session,
                reporter,
                verbose,
                debug,
                wait_for_client,
                use_cache,
                nodes,
                inputs,
                nodes_inputs,
                default_package,
                exclude_packages,
                session_dir,
                project_data,
                pkg_data_root,
                temp_root,
                retain_env_keys,
                env_file,
                bind_paths,
                bind_path_file,
                dry_run,
                report_to_console,
                remote_block_url,
                connector_base_url,
                remote_block_timeout,
            } => {
                assert_eq!(cli.config, "/tmp/oocana.toml");
                assert_eq!(block, "tests/fixtures/connector-flow.oo.yaml");
                assert_eq!(broker.as_deref(), Some("127.0.0.1:47688"));
                assert_eq!(search_paths, vec!["/tmp/a", "/tmp/b"]);
                assert_eq!(session, "session-123");
                assert_eq!(reporter, Some(false));
                assert!(verbose);
                assert_eq!(debug, Some(false));
                assert!(wait_for_client);
                assert!(use_cache);
                assert_eq!(nodes, vec!["node-a", "node-b"]);
                assert_eq!(inputs.as_deref(), Some("{\"input\":1}"));
                assert_eq!(nodes_inputs.as_deref(), Some("{\"node-a\":{\"input\":1}}"));
                assert_eq!(default_package.as_deref(), Some("/pkg/default"));
                assert_eq!(exclude_packages, vec!["/pkg/a", "/pkg/b"]);
                assert_eq!(session_dir.as_deref(), Some("/tmp/session"));
                assert_eq!(project_data, "/tmp/project-data");
                assert_eq!(pkg_data_root, "/tmp/pkg-data");
                assert_eq!(temp_root, "/tmp/temp-root");
                assert_eq!(retain_env_keys, vec!["FOO", "BAR"]);
                assert_eq!(env_file.as_deref(), Some(".env.test"));
                assert_eq!(
                    bind_paths,
                    Some(vec!["src=/src,dst=/dst,ro,recursive".to_string()])
                );
                assert_eq!(bind_path_file.as_deref(), Some("/tmp/binds.txt"));
                assert!(dry_run);
                assert!(report_to_console);
                assert_eq!(remote_block_url.as_deref(), Some("https://remote.example"));
                assert_eq!(
                    connector_base_url.as_deref(),
                    Some("https://connector.example")
                );
                assert_eq!(remote_block_timeout, Some(42));
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn run_command_parses_generated_defaults() {
        let cli = parse_cli(&["oocana", "run", "tests/fixtures/connector-flow.oo.yaml"]);

        match cli.command {
            Commands::Run {
                session,
                reporter,
                debug,
                search_paths,
                nodes,
                exclude_packages,
                retain_env_keys,
                ..
            } => {
                assert!(!session.is_empty());
                assert_eq!(reporter, None);
                assert_eq!(debug, None);
                assert!(search_paths.is_empty());
                assert!(nodes.is_empty());
                assert!(exclude_packages.is_empty());
                assert!(retain_env_keys.is_empty());
            }
            other => panic!("expected run command, got {other:?}"),
        }
    }

    #[test]
    fn query_upstream_parses_all_flags() {
        let cli = parse_cli(&[
            "oocana",
            "query",
            "upstream",
            "examples/base",
            "--nodes",
            "node-a,node-b",
            "--search-paths",
            "/tmp/a,/tmp/b",
            "--use-cache",
        ]);

        match cli.command {
            Commands::Query {
                action:
                    query::QueryAction::Upstream {
                        block,
                        nodes,
                        search_paths,
                        use_cache,
                        ..
                    },
            } => {
                assert_eq!(block, "examples/base");
                assert_eq!(nodes, vec!["node-a", "node-b"]);
                assert_eq!(search_paths, vec!["/tmp/a", "/tmp/b"]);
                assert!(use_cache);
            }
            other => panic!("expected query upstream command, got {other:?}"),
        }
    }

    #[test]
    fn query_other_subcommands_parse() {
        let package = parse_cli(&[
            "oocana",
            "query",
            "package",
            "examples/base",
            "--search-paths",
            "/tmp/pkg-a,/tmp/pkg-b",
            "--use-cache",
        ]);
        match package.command {
            Commands::Query {
                action:
                    query::QueryAction::Package {
                        block,
                        search_paths,
                        use_cache,
                    },
            } => {
                assert_eq!(block, "examples/base");
                assert_eq!(search_paths, vec!["/tmp/pkg-a", "/tmp/pkg-b"]);
                assert!(use_cache);
            }
            other => panic!("expected query package command, got {other:?}"),
        }

        let inputs = parse_cli(&[
            "oocana",
            "query",
            "inputs",
            "examples/base",
            "--search-paths",
            "/tmp/a,/tmp/b",
            "--output",
            "/tmp/inputs.json",
        ]);
        match inputs.command {
            Commands::Query {
                action:
                    query::QueryAction::Inputs {
                        path,
                        search_paths,
                        output,
                    },
            } => {
                assert_eq!(path, "examples/base");
                assert_eq!(search_paths, vec!["/tmp/a", "/tmp/b"]);
                assert_eq!(output.as_deref(), Some("/tmp/inputs.json"));
            }
            other => panic!("expected query inputs command, got {other:?}"),
        }

        let nodes_inputs = parse_cli(&[
            "oocana",
            "query",
            "nodes-inputs",
            "examples/base",
            "--input-types",
            "absence,nullable",
            "--search-paths",
            "/tmp/c,/tmp/d",
            "--output",
            "/tmp/nodes-inputs.json",
        ]);
        match nodes_inputs.command {
            Commands::Query {
                action:
                    query::QueryAction::NodesInputs {
                        flow,
                        input_types,
                        search_paths,
                        output,
                    },
            } => {
                assert_eq!(flow, "examples/base");
                assert_eq!(input_types, vec!["absence", "nullable"]);
                assert_eq!(search_paths, vec!["/tmp/c", "/tmp/d"]);
                assert_eq!(output.as_deref(), Some("/tmp/nodes-inputs.json"));
            }
            other => panic!("expected query nodes-inputs command, got {other:?}"),
        }

        let service = parse_cli(&[
            "oocana",
            "query",
            "service",
            "examples/base",
            "--search-paths",
            "/tmp/svc-a,/tmp/svc-b",
        ]);
        match service.command {
            Commands::Query {
                action:
                    query::QueryAction::Service {
                        block,
                        search_paths,
                    },
            } => {
                assert_eq!(block, "examples/base");
                assert_eq!(search_paths, vec!["/tmp/svc-a", "/tmp/svc-b"]);
            }
            other => panic!("expected query service command, got {other:?}"),
        }
    }

    #[test]
    fn cache_subcommand_parses() {
        let cli = parse_cli(&["oocana", "cache", "clear"]);

        match cli.command {
            Commands::Cache {
                action: cache::CacheAction::Clear {},
            } => {}
            other => panic!("expected cache clear command, got {other:?}"),
        }
    }

    #[test]
    fn package_layer_subcommands_parse() {
        let create = parse_cli(&[
            "oocana",
            "package-layer",
            "create",
            "/pkg/path",
            "--bind-paths",
            "src=/src,dst=/dst,ro,recursive",
            "--bind-path-file",
            "/tmp/binds.txt",
            "--retain-env-keys",
            "FOO",
            "--retain-env-keys",
            "BAR",
            "--env-file",
            ".env.layer",
        ]);
        match create.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::Create {
                        package,
                        bind_paths,
                        bind_path_file,
                        retain_env_keys,
                        env_file,
                    },
            } => {
                assert_eq!(package, "/pkg/path");
                assert_eq!(
                    bind_paths,
                    Some(vec!["src=/src,dst=/dst,ro,recursive".to_string()])
                );
                assert_eq!(bind_path_file.as_deref(), Some("/tmp/binds.txt"));
                assert_eq!(
                    retain_env_keys,
                    Some(vec!["FOO".to_string(), "BAR".to_string()])
                );
                assert_eq!(env_file.as_deref(), Some(".env.layer"));
            }
            other => panic!("expected package-layer create command, got {other:?}"),
        }

        let create_registry = parse_cli(&[
            "oocana",
            "package-layer",
            "create-registry",
            "@pkg/name",
            "1.0.0",
            "/pkg/path",
            "--bind-paths",
            "src=/src,dst=/dst",
            "--bind-path-file",
            "/tmp/registry-binds.txt",
            "--retain-env-keys",
            "TOKEN",
            "--env-file",
            ".env.registry",
        ]);
        match create_registry.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::CreateRegistry {
                        package_name,
                        version,
                        package,
                        bind_paths,
                        bind_path_file,
                        retain_env_keys,
                        env_file,
                    },
            } => {
                assert_eq!(package_name, "@pkg/name");
                assert_eq!(version, "1.0.0");
                assert_eq!(package, "/pkg/path");
                assert_eq!(bind_paths, Some(vec!["src=/src,dst=/dst".to_string()]));
                assert_eq!(bind_path_file.as_deref(), Some("/tmp/registry-binds.txt"));
                assert_eq!(retain_env_keys, Some(vec!["TOKEN".to_string()]));
                assert_eq!(env_file.as_deref(), Some(".env.registry"));
            }
            other => panic!("expected package-layer create-registry command, got {other:?}"),
        }

        let delete = parse_cli(&["oocana", "package-layer", "delete", "/pkg/path"]);
        match delete.command {
            Commands::PackageLayer {
                action: layer::LayerAction::Delete { package },
            } => assert_eq!(package, "/pkg/path"),
            other => panic!("expected package-layer delete command, got {other:?}"),
        }

        let delete_registry = parse_cli(&[
            "oocana",
            "package-layer",
            "delete-registry",
            "@pkg/name",
            "1.0.0",
        ]);
        match delete_registry.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::DeleteRegistry {
                        package_name,
                        version,
                    },
            } => {
                assert_eq!(package_name, "@pkg/name");
                assert_eq!(version, "1.0.0");
            }
            other => panic!("expected package-layer delete-registry command, got {other:?}"),
        }

        let get = parse_cli(&[
            "oocana",
            "package-layer",
            "get",
            "/pkg/path",
            "--package-name",
            "@pkg/name",
            "--version",
            "1.0.0",
        ]);
        match get.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::Get {
                        package,
                        package_name,
                        version,
                    },
            } => {
                assert_eq!(package, "/pkg/path");
                assert_eq!(package_name.as_deref(), Some("@pkg/name"));
                assert_eq!(version.as_deref(), Some("1.0.0"));
            }
            other => panic!("expected package-layer get command, got {other:?}"),
        }

        let get_registry = parse_cli(&[
            "oocana",
            "package-layer",
            "get-registry",
            "@pkg/name",
            "1.0.0",
        ]);
        match get_registry.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::GetRegistry {
                        package_name,
                        version,
                    },
            } => {
                assert_eq!(package_name, "@pkg/name");
                assert_eq!(version, "1.0.0");
            }
            other => panic!("expected package-layer get-registry command, got {other:?}"),
        }

        let scan = parse_cli(&[
            "oocana",
            "package-layer",
            "scan",
            "--search-paths",
            "/tmp/a",
            "--search-paths",
            "/tmp/b",
            "--output",
            "/tmp/scan.json",
        ]);
        match scan.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::Scan {
                        search_paths,
                        output,
                    },
            } => {
                assert_eq!(search_paths, vec!["/tmp/a", "/tmp/b"]);
                assert_eq!(output.as_deref(), Some("/tmp/scan.json"));
            }
            other => panic!("expected package-layer scan command, got {other:?}"),
        }

        let export = parse_cli(&[
            "oocana",
            "package-layer",
            "export",
            "/pkg/path",
            "/tmp/export.tar",
        ]);
        match export.command {
            Commands::PackageLayer {
                action: layer::LayerAction::Export { package, dest },
            } => {
                assert_eq!(package, "/pkg/path");
                assert_eq!(dest, "/tmp/export.tar");
            }
            other => panic!("expected package-layer export command, got {other:?}"),
        }

        let import = parse_cli(&[
            "oocana",
            "package-layer",
            "import",
            "/tmp/export-dir",
            "/pkg/path",
        ]);
        match import.command {
            Commands::PackageLayer {
                action:
                    layer::LayerAction::Import {
                        export_dir,
                        package_path,
                    },
            } => {
                assert_eq!(export_dir, "/tmp/export-dir");
                assert_eq!(package_path, "/pkg/path");
            }
            other => panic!("expected package-layer import command, got {other:?}"),
        }

        let mv = parse_cli(&["oocana", "package-layer", "mv", "/pkg/path"]);
        match mv.command {
            Commands::PackageLayer {
                action: layer::LayerAction::Mv { package },
            } => assert_eq!(package, "/pkg/path"),
            other => panic!("expected package-layer mv command, got {other:?}"),
        }

        let list = parse_cli(&["oocana", "package-layer", "list"]);
        match list.command {
            Commands::PackageLayer {
                action: layer::LayerAction::List {},
            } => {}
            other => panic!("expected package-layer list command, got {other:?}"),
        }

        let delete_all = parse_cli(&["oocana", "package-layer", "delete-all"]);
        match delete_all.command {
            Commands::PackageLayer {
                action: layer::LayerAction::DeleteAll {},
            } => {}
            other => panic!("expected package-layer delete-all command, got {other:?}"),
        }
    }

    #[test]
    fn query_upstream_requires_nodes() {
        let err = Cli::try_parse_from(["oocana", "query", "upstream", "examples/base"])
            .expect_err("query upstream should require --nodes");
        let rendered = err.to_string();
        assert!(rendered.contains("--nodes"));
    }

    #[test]
    fn parse_alias_for_block_search_paths() {
        let cli = parse_cli(&[
            "oocana",
            "run",
            "examples/base",
            "--block-search-paths",
            "/tmp/a,/tmp/b",
        ]);

        match cli.command {
            Commands::Run { search_paths, .. } => {
                assert_eq!(search_paths, vec!["/tmp/a", "/tmp/b"]);
            }
            other => panic!("expected query upstream command, got {other:?}"),
        }
    }
}
