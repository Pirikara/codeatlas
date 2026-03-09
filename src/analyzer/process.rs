use std::collections::{HashMap, HashSet, VecDeque};

/// A detected execution flow (process) through the codebase.
#[derive(Debug, Clone)]
pub struct Process {
    pub id: usize,
    pub label: String,
    pub process_type: ProcessType,
    pub steps: Vec<String>, // ordered symbol UIDs
    #[allow(dead_code)]
    pub entry_point: String,
    #[allow(dead_code)]
    pub terminal: String,
    pub priority: f64, // entry point score
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessType {
    IntraCommunity,
    CrossCommunity,
}

impl std::fmt::Display for ProcessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessType::IntraCommunity => write!(f, "intra_community"),
            ProcessType::CrossCommunity => write!(f, "cross_community"),
        }
    }
}

/// Configuration for process detection.
pub struct ProcessConfig {
    pub max_trace_depth: usize,
    pub max_branching: usize,
    pub max_processes: usize,
    pub min_steps: usize,
    pub min_confidence: f64,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            max_trace_depth: 10,
            max_branching: 4,
            max_processes: 75,
            min_steps: 3,
            min_confidence: 0.5,
        }
    }
}

impl ProcessConfig {
    /// Dynamically size max_processes based on symbol count.
    pub fn for_symbol_count(symbol_count: usize) -> Self {
        let max_processes = (symbol_count / 10).clamp(20, 300);
        Self {
            max_processes,
            ..Default::default()
        }
    }
}

/// Detect execution flows by scoring entry points and tracing BFS paths.
///
/// - `call_edges`: (source_uid, target_uid, confidence)
/// - `symbol_names`: uid -> human-readable name
/// - `community_map`: uid -> community_id (optional, for cross-community detection)
pub fn detect_processes(
    call_edges: &[(String, String, f64)],
    symbol_names: &HashMap<String, String>,
    community_map: &HashMap<String, usize>,
    config: &ProcessConfig,
) -> Vec<Process> {
    // Build adjacency lists (forward CALLS edges above confidence threshold)
    let mut forward: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();

    for (src, tgt, conf) in call_edges {
        if *conf >= config.min_confidence {
            forward.entry(src.as_str()).or_default().push(tgt.as_str());
            reverse.entry(tgt.as_str()).or_default().push(src.as_str());
        }
    }

    // Score entry points (sorted for deterministic iteration)
    let mut entry_scores: Vec<(&str, f64)> = Vec::new();
    let mut all_nodes_sorted: Vec<&str> = forward
        .keys()
        .chain(reverse.keys())
        .copied()
        .collect::<HashSet<&str>>()
        .into_iter()
        .collect();
    all_nodes_sorted.sort();

    for &node in &all_nodes_sorted {
        let score = score_entry_point(node, &forward, &reverse, symbol_names);
        if score > 0.0 {
            entry_scores.push((node, score));
        }
    }

    entry_scores.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(&b.0))
    });
    entry_scores.truncate(200); // top 200 candidates

    // Trace from each entry point
    let mut all_traces: Vec<(Vec<String>, f64)> = Vec::new(); // (trace, entry_score)

    for &(entry, score) in &entry_scores {
        let traces = trace_from_entry(entry, &forward, config);
        for trace in traces {
            if trace.len() >= config.min_steps {
                all_traces.push((trace, score));
            }
        }
    }

    // Deduplicate: remove subset traces
    let all_traces = deduplicate_traces(all_traces);

    // Sort by length (descending), then by entry score
    let mut all_traces = all_traces;
    all_traces.sort_by(|a, b| {
        b.0.len()
            .cmp(&a.0.len())
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
    });
    all_traces.truncate(config.max_processes);

    // Build Process structs
    let mut processes = Vec::new();
    for (i, (trace, priority)) in all_traces.into_iter().enumerate() {
        let entry_point = trace.first().unwrap().clone();
        let terminal = trace.last().unwrap().clone();

        // Determine process type
        let communities: HashSet<usize> = trace
            .iter()
            .filter_map(|uid| community_map.get(uid.as_str()).copied())
            .collect();
        let process_type = if communities.len() > 1 {
            ProcessType::CrossCommunity
        } else {
            ProcessType::IntraCommunity
        };

        // Generate label from first and last step names
        let entry_name = symbol_names
            .get(&entry_point)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let terminal_name = symbol_names
            .get(&terminal)
            .cloned()
            .unwrap_or_else(|| "?".to_string());
        let label = format!("{} → {}", entry_name, terminal_name);

        processes.push(Process {
            id: i,
            label,
            process_type,
            steps: trace,
            entry_point,
            terminal,
            priority,
        });
    }

    processes
}

/// Score a node as a potential entry point.
/// High score = likely entry point (called by few, calls many).
fn score_entry_point(
    uid: &str,
    forward: &HashMap<&str, Vec<&str>>,
    reverse: &HashMap<&str, Vec<&str>>,
    symbol_names: &HashMap<String, String>,
) -> f64 {
    let callee_count = forward.get(uid).map_or(0, |v| v.len()) as f64;
    let caller_count = reverse.get(uid).map_or(0, |v| v.len()) as f64;

    if callee_count == 0.0 {
        return 0.0; // leaf nodes are not entry points
    }

    // Base: calls many / called by few
    let base_score = callee_count / (caller_count + 1.0);

    // Name-based multiplier
    let name = symbol_names.get(uid).map(|s| s.as_str()).unwrap_or("");
    let name_mult = name_multiplier(name);

    // Export multiplier (check uid for exported hint — File: nodes are not entry points)
    let export_mult = if uid.starts_with("File:") {
        0.0 // File nodes are never entry points
    } else {
        1.0
    };

    // Penalize test files
    let test_mult = if is_test_file(uid) { 0.0 } else { 1.0 };

    base_score * name_mult * export_mult * test_mult
}

/// Name pattern matching for entry point detection.
fn name_multiplier(name: &str) -> f64 {
    let lower = name.to_lowercase();

    // Strong entry point patterns
    let entry_patterns = [
        "main", "init", "bootstrap", "start", "run",
        "handle", "dispatch", "execute", "process",
    ];
    for pat in &entry_patterns {
        if lower == *pat || lower.starts_with(pat) {
            return 1.5;
        }
    }

    // Controller/handler suffixes
    let entry_suffixes = ["controller", "handler", "action", "endpoint", "route"];
    for suf in &entry_suffixes {
        if lower.ends_with(suf) {
            return 1.5;
        }
    }

    // Framework-specific patterns
    // Express/HTTP handlers
    if lower.starts_with("get") || lower.starts_with("post") || lower.starts_with("put") || lower.starts_with("delete") || lower.starts_with("patch") {
        // Only if it looks like a route handler, not a getter
        if name.len() > 4 && name.chars().nth(3).map_or(false, |c| c.is_uppercase()) {
            return 1.3;
        }
    }

    // React hooks (use*)
    if lower.starts_with("use") && name.len() > 3 && name.chars().nth(3).map_or(false, |c| c.is_uppercase()) {
        return 1.2;
    }

    // Utility patterns — penalize
    let util_prefixes = [
        "get", "set", "is", "has", "can", "should", "will", "did",
        "format", "parse", "validate", "convert", "transform",
        "encode", "decode", "serialize", "deserialize",
        "to_", "from_",
    ];
    for prefix in &util_prefixes {
        if lower.starts_with(prefix) {
            return 0.3;
        }
    }

    let util_suffixes = ["helper", "util", "utils"];
    for suf in &util_suffixes {
        if lower.ends_with(suf) {
            return 0.3;
        }
    }

    1.0
}

/// Check if a UID corresponds to a test file.
fn is_test_file(uid: &str) -> bool {
    let lower = uid.to_lowercase();
    lower.contains(".test.") || lower.contains(".spec.")
        || lower.contains("__tests__") || lower.contains("/test/")
        || lower.contains("/tests/") || lower.contains("_test.")
        || lower.contains("_spec.")
}

/// BFS trace from an entry point, collecting all paths to terminal nodes.
fn trace_from_entry(
    entry: &str,
    forward: &HashMap<&str, Vec<&str>>,
    config: &ProcessConfig,
) -> Vec<Vec<String>> {
    let mut results: Vec<Vec<String>> = Vec::new();
    let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();

    queue.push_back((entry.to_string(), vec![entry.to_string()]));

    let mut iterations = 0;
    let max_iterations = 10_000; // safety limit

    while let Some((current, path)) = queue.pop_front() {
        iterations += 1;
        if iterations > max_iterations {
            break;
        }

        if path.len() > config.max_trace_depth {
            results.push(path);
            continue;
        }

        let neighbors = forward.get(current.as_str());
        let neighbors = match neighbors {
            Some(n) if !n.is_empty() => n,
            _ => {
                // Terminal node
                results.push(path);
                continue;
            }
        };

        // Filter out cycles and limit branching
        let valid: Vec<&&str> = neighbors
            .iter()
            .filter(|n| !path.contains(&n.to_string()))
            .take(config.max_branching)
            .collect();

        if valid.is_empty() {
            // All neighbors are cycles → terminal
            results.push(path);
            continue;
        }

        for &&next in &valid {
            let mut new_path = path.clone();
            new_path.push(next.to_string());
            queue.push_back((next.to_string(), new_path));
        }
    }

    results
}

/// Remove subset traces and deduplicate paths between same endpoints.
fn deduplicate_traces(traces: Vec<(Vec<String>, f64)>) -> Vec<(Vec<String>, f64)> {
    if traces.is_empty() {
        return traces;
    }

    // Convert traces to sets for subset checking
    let trace_sets: Vec<HashSet<&str>> = traces
        .iter()
        .map(|(t, _)| t.iter().map(|s| s.as_str()).collect())
        .collect();

    // Mark subsets
    let mut keep = vec![true; traces.len()];
    for i in 0..traces.len() {
        if !keep[i] {
            continue;
        }
        for j in 0..traces.len() {
            if i == j || !keep[j] {
                continue;
            }
            // If j is a subset of i and shorter, remove j
            if traces[j].0.len() < traces[i].0.len() && trace_sets[j].is_subset(&trace_sets[i]) {
                keep[j] = false;
            }
        }
    }

    // Keep longest trace per (entry, terminal) pair
    let mut best_per_pair: HashMap<(&str, &str), (usize, usize)> = HashMap::new(); // (entry,terminal) -> (index, length)
    for (i, (trace, _)) in traces.iter().enumerate() {
        if !keep[i] || trace.is_empty() {
            continue;
        }
        let key = (trace.first().unwrap().as_str(), trace.last().unwrap().as_str());
        match best_per_pair.get(&key) {
            Some(&(_, best_len)) if trace.len() <= best_len => {
                keep[i] = false;
            }
            Some(&(prev_idx, _)) => {
                keep[prev_idx] = false;
                best_per_pair.insert(key, (i, trace.len()));
            }
            None => {
                best_per_pair.insert(key, (i, trace.len()));
            }
        }
    }

    traces
        .into_iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, t)| t)
        .collect()
}
