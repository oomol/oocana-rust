use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
pub struct Args {
    #[clap(long)]
    pub address: String,

    #[clap(long)]
    pub pipeline_task_id: String,

    #[clap(long)]
    pub block_task_id: String,

    #[clap(long)]
    pub block_id: String,
}
