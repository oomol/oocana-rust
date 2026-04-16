use crate::fun::arg::parse_search_paths;
use clap::Subcommand;
use manifest_meta::{BlockResolver, read_flow_or_block};
use manifest_reader::path_finder::BlockPathFinder;
use one_shot::one_shot::{UpstreamArgs, find_upstream};
use std::collections::HashSet;
use std::env;
use std::io::Write;
use utils::error::Result;

#[derive(Debug, Subcommand)]
pub enum QueryAction {
    #[command(about = "query upstream")]
    Upstream {
        #[arg(
            help = "Absolute Path to the Oocana Block Manifest file or a directory with flow.oo.yaml."
        )]
        block: String,
        #[arg(
            help = "Stop the flow after the listed nodes are finished. Repeat the flag or use commas.",
            long,
            required = true,
            value_delimiter = ','
        )]
        nodes: Vec<String>,
        #[arg(
            help = "Paths to search for blocks. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
    },
    #[command(about = "get package layers from a flow block")]
    Package {
        block: String,
        #[arg(
            help = "Paths to search for blocks. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
        // #[arg(help = "Stop the flow after the node is finished.", long)]
        // nodes: Option<HashSet<String>>,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
    },
    #[command(about = "query block(task, subflow)'s inputs")]
    Inputs {
        #[arg(help = "path to the block, it can be a directory or file path.")]
        path: String,
        #[arg(
            help = "Paths to search for blocks. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
        #[arg(
            help = "output file path (JSON format), if not provided, it will print to stdout",
            long
        )]
        output: Option<String>,
    },
    #[command(about = "query flow block's all start inputs(no connection)")]
    NodesInputs {
        #[arg(help = "path to the flow block, it can be a directory or file path.")]
        flow: String,
        #[arg(
            help = "filter input types, e.g. 'all', 'absence', 'nullable', if not provided, it will return all inputs.",
            long,
            alias = "input-types",
            value_delimiter = ','
        )]
        input_types: Vec<String>,
        #[arg(
            help = "Paths to search for blocks. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
        #[arg(
            help = "output file path (JSON format), if not provided, it will print to stdout",
            long
        )]
        output: Option<String>,
    },
    #[command(
        about = "get services from a flow block. will output service struct line by line, output will be like: 'service: {json-style service struct}'"
    )]
    Service {
        #[arg(
            help = "Absolute Path to the Oocana Block Manifest file or a directory with flow.oo.yaml."
        )]
        block: String,
        #[arg(
            help = "Paths to search for blocks. Repeat the flag or use commas. Fallback to config/current flow block.",
            long,
            alias = "block-search-paths",
            value_delimiter = ','
        )]
        search_paths: Vec<String>,
    },
}

fn query_context(search_paths: &[String]) -> Result<(BlockResolver, BlockPathFinder)> {
    let search_paths = parse_search_paths(search_paths);
    let cwd = env::current_dir()?;
    Ok((
        BlockResolver::new(),
        BlockPathFinder::new(cwd, search_paths),
    ))
}

fn write_json_output(
    output: &Option<String>,
    json_result: &str,
    success_message: &str,
) -> Result<()> {
    if let Some(output) = output {
        let mut file = std::fs::File::create(output)?;
        write!(file, "{json_result}")?;
        file.flush()?;
        println!("{success_message}: {output}");
    } else {
        println!("{json_result}");
    }
    Ok(())
}

pub fn query(action: &QueryAction) -> Result<()> {
    match action {
        QueryAction::Upstream {
            block,
            nodes,
            search_paths,
            use_cache,
        } => {
            let (r, w, whole) = find_upstream(UpstreamArgs {
                block_path: block,
                search_paths: parse_search_paths(search_paths),
                use_cache: use_cache.to_owned(),
                nodes: Some(nodes.iter().cloned().collect::<HashSet<String>>()),
            })?;
            println!(
                "run:{}\nwaiting:{}\nwhole:{}",
                r.join(","),
                w.join(","),
                whole.join(",")
            );
        }
        QueryAction::Package {
            block,
            search_paths,
            use_cache: _,
        } => {
            let search_paths = parse_search_paths(search_paths);
            let path_finder = BlockPathFinder::new(env::current_dir()?, search_paths);
            let package_status = runtime::get_packages(runtime::GetPackageArgs {
                block,
                block_reader: BlockResolver::new(),
                path_finder,
                nodes: Some(HashSet::new()),
            })?;
            for (package, layer) in package_status {
                println!("package-status: {package:?}:{layer:?}");
            }
        }
        QueryAction::Inputs {
            path,
            search_paths,
            output,
        } => {
            let (mut block_reader, mut block_path_finder) = query_context(search_paths)?;
            let block_or_flow =
                read_flow_or_block(path, &mut block_reader, &mut block_path_finder)?;
            let inputs = block_or_flow.inputs_def();
            let json_result = serde_json::to_string(&inputs)?;
            write_json_output(output, &json_result, "inputs written to file")?;
        }
        QueryAction::NodesInputs {
            flow,
            search_paths,
            input_types: _, // todo: support input types filter
            output,
        } => {
            let (mut block_reader, mut block_path_finder) = query_context(search_paths)?;
            let block_or_flow =
                read_flow_or_block(flow, &mut block_reader, &mut block_path_finder)?;
            match block_or_flow {
                manifest_meta::Block::Flow(flow) => {
                    let input = flow.read().unwrap().query_nodes_inputs();
                    let json_result = serde_json::to_string(&input)?;
                    write_json_output(output, &json_result, "blank input written to file")?;
                }
                _ => {
                    println!("block is not flow");
                    return Err(utils::error::Error::new(
                        "Block is not a flow, cannot get blank input.",
                    ));
                }
            }
        }
        QueryAction::Service {
            block,
            search_paths,
        } => {
            let (mut block_reader, mut block_path_finder) = query_context(search_paths)?;
            let block_or_flow =
                read_flow_or_block(block, &mut block_reader, &mut block_path_finder)?;

            match block_or_flow {
                manifest_meta::Block::Flow(flow) => {
                    let results = flow.read().unwrap().get_services();
                    for result in results {
                        println!("service: {result:}");
                    }
                }
                _ => {
                    println!("block is not flow");
                }
            }
        }
    }
    Ok(())
}
