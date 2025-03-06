use crate::graph::{Block, Graph, GraphSerializable};
use crate::pkg_finder::PkgFinder;
use crate::sink::{BlockInputSink, BlockTaskSink, PipelineTask};
use mainframe::{ClientMessage, MBlockInput, ServerMessage};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::json;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;
use utils::error::Result;

use std::process;

#[derive(Deserialize, Debug)]
struct BlockMeta {
    pub exec: String,
}

fn read_json_file<T>(file_path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let json_file = File::open(file_path)?;
    let json_reader = BufReader::new(json_file);
    let json_data: T = serde_json::from_reader(json_reader)?;
    Ok(json_data)
}

fn run_worker(
    pkg_finder: &mut PkgFinder, address: &str, pipeline_task_id: &str, block: &Block,
    block_task_id: &str,
) {
    // TODO cache pkg_path
    let pkg_path = pkg_finder
        .find_pkg(&block.pkg)
        .expect(format!("Cannot find package {:?}", &block.pkg).as_str());

    let block_meta = read_json_file::<BlockMeta>(&pkg_path.join("block.json"))
        .expect(format!("Cannot read block.json of package {:?}", &block.pkg).as_str());

    let mut args = block_meta.exec.split_whitespace().collect::<Vec<&str>>();
    // First argument is the path to the executable
    let exec = args.remove(0);

    // add block task arguments
    args.extend(
        [
            "--address",
            address,
            "--pipeline-task-id",
            pipeline_task_id,
            "--block-task-id",
            block_task_id,
            "--block-id",
            &block.id,
        ]
        .iter(),
    );

    // Execute the command
    process::Command::new(exec)
        .current_dir(pkg_path.as_os_str())
        .args(args)
        .spawn()
        .expect("Failed to start Vocana client");
}

pub fn run_pipeline(graph_path: &str) -> Result<()> {
    let mut graph_file_path = Path::new(graph_path).to_path_buf();

    if graph_file_path.is_dir() {
        graph_file_path.push("graph.json")
    }

    let graph_root_path = graph_file_path.parent().unwrap();

    let pipeline_graph = Graph::new(
        read_json_file::<GraphSerializable>(&graph_file_path).expect("Cannot read graph.json"),
    );

    let mut pipeline_task = PipelineTask::new(&pipeline_graph.id);

    let mut block_input_sink = BlockInputSink::new(&pipeline_task.task_id);
    let mut block_task_sink = BlockTaskSink::new(&pipeline_task.task_id);

    let address = format!("ipc:///tmp/vocana-{}", &pipeline_task.task_id);

    let mut pkg_finder = PkgFinder::new(graph_root_path.to_path_buf());

    let mainframe_server = mainframe::Server::new();
    mainframe_server
        .bind(&address)
        .expect("Cannot connect to socket");

    pipeline_graph.find_init_blocks().iter().for_each(|block| {
        let block_task_id = block_task_sink.create(&block.id);

        run_worker(
            &mut pkg_finder,
            &address,
            &pipeline_task.task_id,
            block,
            &block_task_id,
        );
    });

    mainframe_server.on_msg(|msg| match msg {
        ClientMessage::BlockReady(msg) => Some(ServerMessage::BlockInput(MBlockInput {
            pipeline_task_id: msg.pipeline_task_id.to_owned(),
            block_task_id: msg.block_task_id.to_owned(),
            input: block_task_sink
                .get_block_task(&msg.block_task_id)
                .and_then(|block_task| {
                    block_task
                        .inputs
                        .as_ref()
                        .and_then(|inputs| Some(json!(inputs.clone())))
                }),
            options: pipeline_graph.get_block(&msg.block_id).and_then(|block| {
                block
                    .options
                    .as_ref()
                    .and_then(|options| Some(options.clone()))
            }),
        })),
        ClientMessage::BlockOutput(msg) => {
            if msg.done {
                block_task_sink.done(&msg.block_task_id);
                block_task_sink.remove(&msg.block_task_id);
            }

            let shared_output = Rc::new(msg.output);
            for edge in pipeline_graph
                .find_out_edges(&msg.block_id, Some(&msg.slot_id))
                .into_iter()
            {
                if let Some(to_block) = pipeline_graph.get_block(&edge.to_block) {
                    block_input_sink.add_block_input(
                        &edge.to_block,
                        &edge.to_slot,
                        Rc::clone(&shared_output),
                    );

                    while let Some(input) =
                        block_input_sink.pop_block_inputs(&to_block.id, &to_block.in_slots)
                    {
                        let block_task_id = block_task_sink.create(&edge.to_block);
                        block_task_sink.update_inputs(&block_task_id, Some(input.clone()));

                        run_worker(
                            &mut pkg_finder,
                            &address,
                            &pipeline_task.task_id,
                            to_block,
                            &block_task_id,
                        );
                    }
                }
            }

            if block_task_sink.active_block_tasks.len() <= 0 {
                pipeline_task.done();
                process::exit(0);
            }

            None
        }
        ClientMessage::BlockDone(msg) => {
            block_task_sink.done(&msg.block_task_id);
            block_task_sink.remove(&msg.block_task_id);

            for edge in pipeline_graph
                .find_out_edges(&msg.block_id, None)
                .into_iter()
            {
                if let Some(to_block) = pipeline_graph.get_block(&edge.to_block) {
                    while let Some(input) =
                        block_input_sink.pop_block_inputs(&to_block.id, &to_block.in_slots)
                    {
                        let block_task_id = block_task_sink.create(&edge.to_block);
                        block_task_sink.update_inputs(&block_task_id, Some(input.clone()));

                        run_worker(
                            &mut pkg_finder,
                            &address,
                            &pipeline_task.task_id,
                            to_block,
                            &block_task_id,
                        );
                    }
                }
            }

            if block_task_sink.active_block_tasks.len() <= 0 {
                pipeline_task.done();
                process::exit(0);
            }

            None
        }
        _ => None,
    })?;

    Ok(())
}
