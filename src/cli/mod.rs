pub mod clusters_cmd;
pub mod context_cmd;
pub mod embed_cmd;
pub mod eval_cmd;
pub mod graph_query_cmd;
pub mod impact_batch_cmd;
pub mod impact_cmd;
pub mod index;
pub mod metrics;
pub mod processes_cmd;
pub mod query_cmd;
pub mod status;
pub mod subgraph_cmd;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codeatlas", version, about = "Code knowledge graph builder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Output JSON instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum SearchMode {
    Bm25,
    Vector,
    Hybrid,
}

#[derive(clap::ValueEnum, Clone, Debug, PartialEq)]
pub enum ImpactDirection {
    Upstream,
    Downstream,
}

#[derive(Subcommand)]
pub enum Command {
    /// Analyze and index a repository
    Index(IndexArgs),
    /// Show index status
    Status(StatusArgs),
    /// Search symbols by keyword (FTS5 BM25)
    Query(QueryArgs),
    /// Show 360-degree context for a symbol
    Context(ContextArgs),
    /// Analyze blast radius of changing a symbol
    Impact(ImpactArgs),
    /// List detected communities/clusters
    Clusters(ClustersArgs),
    /// List detected execution flows
    Processes(ProcessesArgs),
    /// Generate embeddings for indexed symbols
    Embed(EmbedArgs),
    /// Evaluate search quality against a fixture
    Eval(EvalArgs),
    /// Get reachable subgraph from a symbol (nodes + edges)
    Subgraph(SubgraphArgs),
    /// Analyze impact for a batch of symbols or file ranges (VCS-independent)
    ImpactBatch(ImpactBatchArgs),
    /// Execute a read-only SQL SELECT query against the knowledge graph
    GraphQuery(GraphQueryArgs),
}

#[derive(clap::Args)]
pub struct IndexArgs {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Force full re-index (ignore cache)
    #[arg(long)]
    pub force: bool,

    /// Print per-phase timing, failure counts, and memory usage to stderr
    #[arg(long)]
    pub metrics: bool,
}

#[derive(clap::Args)]
pub struct StatusArgs {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

#[derive(clap::Args)]
pub struct QueryArgs {
    /// Search query
    pub query: String,

    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Max results
    #[arg(short, long, default_value = "10")]
    pub limit: usize,

    /// Search mode: bm25, vector, or hybrid
    #[arg(long, value_enum, default_value = "bm25", conflicts_with = "grouped")]
    pub mode: SearchMode,

    /// Return results grouped by execution process
    #[arg(long, default_value = "false")]
    pub grouped: bool,
}

#[derive(clap::Args)]
pub struct ContextArgs {
    /// Symbol name to inspect (required unless --uid is given)
    pub name: Option<String>,

    /// Direct UID lookup (zero-ambiguity); conflicts with positional name
    #[arg(long, conflicts_with = "name")]
    pub uid: Option<String>,

    /// Narrow down by file path when the name is ambiguous
    #[arg(long)]
    pub file: Option<String>,

    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,
}

#[derive(clap::Args)]
pub struct ImpactArgs {
    /// Symbol name to analyze
    pub name: String,

    /// Direction: "upstream" (callers) or "downstream" (callees)
    #[arg(short, long, default_value = "upstream")]
    pub direction: String,

    /// Max traversal depth
    #[arg(long, default_value = "3")]
    pub depth: u32,

    /// Minimum confidence threshold
    #[arg(long, default_value = "0.5")]
    pub min_confidence: f64,

    /// Limit traversal to CALLS relationships only (excludes IMPORTS, CONTAINS, etc.)
    #[arg(long)]
    pub calls_only: bool,

    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,
}

#[derive(clap::Args)]
pub struct EmbedArgs {
    /// Path to the repository (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Force re-generation of all embeddings (ignore cache)
    #[arg(long)]
    pub force: bool,
}

#[derive(clap::Args)]
pub struct ClustersArgs {
    /// Path to the repository
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

#[derive(clap::Args)]
pub struct ProcessesArgs {
    /// Path to the repository
    #[arg(default_value = ".")]
    pub path: PathBuf,
}

#[derive(clap::Args)]
pub struct SubgraphArgs {
    /// Symbol name to explore (required unless --uid or --id is given)
    pub name: Option<String>,

    /// Direct UID lookup (zero-ambiguity); conflicts with name, file
    #[arg(long, conflicts_with_all = ["name", "file"])]
    pub uid: Option<String>,

    /// Direct integer ID lookup; conflicts with name, file, uid
    #[arg(long, conflicts_with_all = ["name", "file", "uid"])]
    pub id: Option<i64>,

    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: std::path::PathBuf,

    /// Narrow down by file path when the name is ambiguous (requires name)
    #[arg(long, requires = "name")]
    pub file: Option<String>,

    /// Direction: "outgoing", "incoming", or "both"
    #[arg(long, default_value = "outgoing")]
    pub direction: String,

    /// Max traversal depth
    #[arg(long, default_value = "3")]
    pub depth: u32,

    /// Edge types to follow (comma-separated; empty = all)
    #[arg(long, value_delimiter = ',')]
    pub edge_types: Vec<String>,

    /// Max nodes to return
    #[arg(long, default_value = "100")]
    pub max_nodes: usize,

    /// Max edges to return
    #[arg(long, default_value = "500")]
    pub max_edges: usize,
}

#[derive(clap::Args)]
pub struct EvalArgs {
    /// Path to the eval fixture JSON file
    pub fixture: PathBuf,

    /// Number of top results to consider
    #[arg(short, long, default_value = "5")]
    pub k: usize,

    /// Search mode (omit to default to bm25)
    #[arg(long, value_enum, conflicts_with_all = ["grouped", "all"])]
    pub mode: Option<SearchMode>,

    /// Run all modes (bm25, vector, hybrid)
    #[arg(long, conflicts_with_all = ["mode", "grouped"])]
    pub all: bool,

    /// Evaluate using process-grouped search
    #[arg(long, conflicts_with_all = ["mode", "all"])]
    pub grouped: bool,

    /// Write JSON report to this file
    #[arg(long)]
    pub output_file: Option<PathBuf>,

    /// Fail if BM25 Recall@k is below this threshold
    #[arg(long)]
    pub min_recall: Option<f64>,

    /// Fail if BM25 MRR is below this threshold
    #[arg(long)]
    pub min_mrr: Option<f64>,

    /// Fail if GroupedRoutingAccuracy is below this threshold (requires --grouped)
    #[arg(long, requires = "grouped")]
    pub min_process_hit: Option<f64>,
}

#[derive(clap::Args)]
pub struct ImpactBatchArgs {
    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Symbol entries as JSON: [{"id":123}, {"name":"Foo","file":"a.go"}]
    #[arg(long)]
    pub symbols: Option<String>,

    /// File ranges as JSON: [{"file":"a.go","start":10,"end":20}]
    #[arg(long)]
    pub ranges: Option<String>,

    /// Impact direction to analyze
    #[arg(long, value_enum, default_value = "upstream")]
    pub direction: ImpactDirection,

    /// Max traversal depth for impact analysis
    #[arg(long, default_value = "3")]
    pub depth: u32,

    /// Minimum confidence threshold
    #[arg(long, default_value = "0.5")]
    pub min_confidence: f64,

    /// Limit impact traversal to CALLS relationships only
    #[arg(long)]
    pub calls_only: bool,

    /// Max symbols to return
    #[arg(long, default_value = "20")]
    pub max_symbols: usize,

    /// Symbol kinds to include (comma-separated; default when omitted: Function,Method; empty = all)
    #[arg(long, value_delimiter = ',', conflicts_with = "all_kinds")]
    pub kinds: Option<Vec<String>>,

    /// Include all symbol kinds regardless of --kinds
    #[arg(long)]
    pub all_kinds: bool,
}

#[derive(clap::Args)]
pub struct GraphQueryArgs {
    /// SQL SELECT query to execute (read-only; SELECT and WITH only)
    pub query: String,

    /// Path to the repository
    #[arg(short, long, default_value = ".")]
    pub path: PathBuf,

    /// Maximum rows to return
    #[arg(long, default_value = "200")]
    pub limit: usize,
}
