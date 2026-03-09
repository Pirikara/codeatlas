mod analyzer;
mod cli;
mod embedder;
mod eval;
mod parser;
pub mod query;
mod scanner;
mod storage;

use clap::Parser;
use cli::{Cli, Command};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let json = cli.json;
    match cli.command {
        Command::Index(args) => cli::index::run(args),
        Command::Status(args) => cli::status::run(args, json),
        Command::Query(args) => cli::query_cmd::run(args, json),
        Command::Context(args) => cli::context_cmd::run(args, json),
        Command::Impact(args) => cli::impact_cmd::run(args, json),
        Command::Clusters(args) => cli::clusters_cmd::run(args, json),
        Command::Processes(args) => cli::processes_cmd::run(args, json),
        Command::Embed(args) => cli::embed_cmd::run(args),
        Command::Eval(args) => cli::eval_cmd::run(args, json),
        Command::Subgraph(args) => cli::subgraph_cmd::run(args, json),
        Command::ImpactBatch(args) => cli::impact_batch_cmd::run(args, json),
        Command::GraphQuery(args) => cli::graph_query_cmd::run(args, json),
    }
}
