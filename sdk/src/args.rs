use clap::Parser;

/// Simple program to greet a person
#[derive(Parser, Debug)]
pub struct Args {
    #[clap(long)]
    pub address: String,

    #[clap(long)]
    pub session_id: String,

    #[clap(long)]
    pub job_id: String,
}
