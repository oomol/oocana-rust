use crate::fun::arg::parse_search_paths;
use clap::Subcommand;
use manifest_meta::{read_flow_or_block, BlockResolver};
use manifest_reader::path_finder::BlockPathFinder;
use one_shot::one_shot::{find_upstream, UpstreamArgs};
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
        #[arg(help = "Stop the flow after the node is finished.", long)]
        nodes: String,
        #[arg(
            help = "Paths to search for blocks. Fallback to the directory of current flow block.",
            long,
            alias = "block-search-paths"
        )]
        search_paths: Option<String>,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
    },
    #[command(about = "get package layers from a flow block")]
    Package {
        block: String,
        #[arg(
            help = "Paths to search for blocks. Fallback to the directory of current flow block.",
            long,
            alias = "block-search-paths"
        )]
        search_paths: Option<String>,
        // #[arg(help = "Stop the flow after the node is finished.", long)]
        // nodes: Option<HashSet<String>>,
        #[arg(help = "Use previous result cache if exist.", long)]
        use_cache: bool,
    },
    #[command(about = "query a flow block's absence input")]
    Input {
        #[arg(
            help = "Absolute Path to the Oocana Block Manifest file or a directory with flow.oo.yaml."
        )]
        block: String,
        #[arg(
            help = "Paths to search for blocks. Fallback to the directory of current flow block.",
            long,
            alias = "block-search-paths"
        )]
        search_paths: Option<String>,
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
            help = "Paths to search for blocks. Fallback to the directory of current flow block.",
            long,
            alias = "block-search-paths"
        )]
        search_paths: Option<String>,
    },
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
                nodes: Some(
                    nodes
                        .split(',')
                        .map(|node| node.to_string())
                        .collect::<HashSet<String>>(),
                ),
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

            let block_reader = BlockResolver::new();
            let path_finder = BlockPathFinder::new(env::current_dir().unwrap(), search_paths);
            let package_status = runtime::get_packages(runtime::GetPackageArgs {
                block,
                block_reader,
                path_finder,
                nodes: Some(HashSet::new()),
            })?;
            for (package, layer) in package_status {
                println!("package-status: {:?}:{:?}", package, layer);
            }
        }
        QueryAction::Input {
            block,
            search_paths,
            output,
        } => {
            let block_reader = BlockResolver::new();
            let block_path_finder = BlockPathFinder::new(
                env::current_dir().unwrap(),
                parse_search_paths(search_paths),
            );
            let block_or_flow = read_flow_or_block(block, block_reader, block_path_finder)?;
            match block_or_flow {
                manifest_meta::Block::Flow(flow) => {
                    let input = flow.get_absence_input();
                    let json_result = serde_json::to_string(&input)?;
                    if let Some(output) = output {
                        let mut file = std::fs::File::create(output)?;
                        write!(file, "{}", json_result)?;
                        file.flush()?;
                        println!("blank input written to file: {}", output);
                    } else {
                        println!("{}", json_result);
                    }
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
            let block_reader = BlockResolver::new();
            let block_path_finder = BlockPathFinder::new(
                env::current_dir().unwrap(),
                parse_search_paths(search_paths),
            );

            let block_or_flow = read_flow_or_block(block, block_reader, block_path_finder)?;

            match block_or_flow {
                manifest_meta::Block::Flow(flow) => {
                    let results = flow.get_services();
                    for result in results {
                        println!("service: {:}", result);
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
