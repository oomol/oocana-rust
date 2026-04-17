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

    let create_external = parse_cli(&[
        "oocana",
        "package-layer",
        "create-external",
        "@pkg/name",
        "1.0.0",
        "/pkg/path",
        "--bind-paths",
        "src=/src,dst=/dst",
        "--bind-path-file",
        "/tmp/external-binds.txt",
        "--retain-env-keys",
        "TOKEN",
        "--env-file",
        ".env.external",
    ]);
    match create_external.command {
        Commands::PackageLayer {
            action:
                layer::LayerAction::CreateExternal {
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
            assert_eq!(bind_path_file.as_deref(), Some("/tmp/external-binds.txt"));
            assert_eq!(retain_env_keys, Some(vec!["TOKEN".to_string()]));
            assert_eq!(env_file.as_deref(), Some(".env.external"));
        }
        other => panic!("expected package-layer create-external command, got {other:?}"),
    }

    let delete = parse_cli(&["oocana", "package-layer", "delete", "/pkg/path"]);
    match delete.command {
        Commands::PackageLayer {
            action: layer::LayerAction::Delete { package },
        } => assert_eq!(package, "/pkg/path"),
        other => panic!("expected package-layer delete command, got {other:?}"),
    }

    let delete_external = parse_cli(&[
        "oocana",
        "package-layer",
        "delete-external",
        "@pkg/name",
        "1.0.0",
    ]);
    match delete_external.command {
        Commands::PackageLayer {
            action:
                layer::LayerAction::DeleteExternal {
                    package_name,
                    version,
                },
        } => {
            assert_eq!(package_name, "@pkg/name");
            assert_eq!(version, "1.0.0");
        }
        other => panic!("expected package-layer delete-external command, got {other:?}"),
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

    let get_external = parse_cli(&[
        "oocana",
        "package-layer",
        "get-external",
        "@pkg/name",
        "1.0.0",
    ]);
    match get_external.command {
        Commands::PackageLayer {
            action:
                layer::LayerAction::GetExternal {
                    package_name,
                    version,
                },
        } => {
            assert_eq!(package_name, "@pkg/name");
            assert_eq!(version, "1.0.0");
        }
        other => panic!("expected package-layer get-external command, got {other:?}"),
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
