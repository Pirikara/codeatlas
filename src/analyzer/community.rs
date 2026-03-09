use std::collections::{BTreeMap, HashMap, HashSet};

/// A detected community (cluster) of symbols.
#[derive(Debug, Clone)]
pub struct Community {
    pub id: usize,
    pub label: String,
    pub members: Vec<String>, // symbol UIDs
    pub cohesion: f64,
}

/// Run community detection on a graph of symbols connected by edges.
/// Uses a simplified Louvain/Leiden-style modularity optimization.
///
/// Input: list of (source_uid, target_uid) edges.
/// Output: list of communities with member UIDs and heuristic labels.
pub fn detect_communities(
    edges: &[(String, String)],
    symbol_names: &HashMap<String, String>, // uid -> name
) -> Vec<Community> {
    if edges.is_empty() {
        return vec![];
    }

    // Build adjacency list and collect all nodes
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut nodes: HashSet<&str> = HashSet::new();

    for (src, tgt) in edges {
        adj.entry(src.as_str()).or_default().push(tgt.as_str());
        adj.entry(tgt.as_str()).or_default().push(src.as_str());
        nodes.insert(src.as_str());
        nodes.insert(tgt.as_str());
    }

    let mut node_list: Vec<&str> = nodes.into_iter().collect();
    node_list.sort();
    let total_edges = edges.len() as f64 * 2.0; // each edge counted twice (undirected)

    // Initialize: each node in its own community
    let mut community_of: HashMap<&str, usize> = HashMap::new();
    for (i, &node) in node_list.iter().enumerate() {
        community_of.insert(node, i);
    }

    // Degree of each node
    let degree: HashMap<&str, f64> = node_list
        .iter()
        .map(|&n| (n, adj.get(n).map_or(0, |v| v.len()) as f64))
        .collect();

    // Louvain phase 1: repeatedly move nodes to maximize modularity
    let mut improved = true;
    let mut iterations = 0;
    while improved && iterations < 20 {
        improved = false;
        iterations += 1;

        for &node in &node_list {
            let current_comm = community_of[node];
            let node_deg = degree[node];

            // Calculate edges to each neighboring community (BTreeMap for deterministic iteration)
            let mut comm_edges: BTreeMap<usize, f64> = BTreeMap::new();
            if let Some(neighbors) = adj.get(node) {
                for &nbr in neighbors {
                    let nbr_comm = community_of[nbr];
                    *comm_edges.entry(nbr_comm).or_default() += 1.0;
                }
            }

            // Calculate community totals (sum of degrees)
            let mut comm_totals: HashMap<usize, f64> = HashMap::new();
            for (&n, &comm) in &community_of {
                *comm_totals.entry(comm).or_default() += degree[n];
            }

            // Find best community to move to
            let mut best_comm = current_comm;
            let mut best_delta = 0.0;

            // Modularity gain of removing node from current community
            let ki_in_current = comm_edges.get(&current_comm).copied().unwrap_or(0.0);
            let sigma_current = comm_totals.get(&current_comm).copied().unwrap_or(0.0);

            for (&target_comm, &ki_in) in &comm_edges {
                if target_comm == current_comm {
                    continue;
                }
                let sigma_target = comm_totals.get(&target_comm).copied().unwrap_or(0.0);

                // Modularity gain = [ki_in / m - sigma_tot * ki / (2m^2)]
                //                  - [ki_in_current / m - (sigma_current - ki) * ki / (2m^2)]
                let delta = (ki_in - ki_in_current) / total_edges
                    + node_deg * (sigma_current - sigma_target - node_deg)
                        / (total_edges * total_edges);

                if delta > best_delta || (delta == best_delta && target_comm < best_comm) {
                    best_delta = delta;
                    best_comm = target_comm;
                }
            }

            if best_comm != current_comm {
                community_of.insert(node, best_comm);
                improved = true;
            }
        }
    }

    // Collect communities
    let mut comm_members: HashMap<usize, Vec<String>> = HashMap::new();
    for (&node, &comm) in &community_of {
        comm_members
            .entry(comm)
            .or_default()
            .push(node.to_string());
    }

    // Filter out singleton communities (just one node, not interesting)
    let mut communities: Vec<Community> = Vec::new();
    let mut id = 0;

    // Sort by community key for deterministic ID assignment
    let mut sorted_comms: Vec<(usize, Vec<String>)> = comm_members.into_iter().collect();
    sorted_comms.sort_by_key(|(k, _)| *k);

    for (_, mut members) in sorted_comms {
        if members.len() < 2 {
            continue;
        }
        // Sort members within each community for deterministic output
        members.sort();

        // Calculate cohesion: internal edges / possible edges
        let member_set: HashSet<&str> = members.iter().map(|s| s.as_str()).collect();
        let mut internal_edges = 0;
        for &(ref src, ref tgt) in edges {
            if member_set.contains(src.as_str()) && member_set.contains(tgt.as_str()) {
                internal_edges += 1;
            }
        }
        let possible = members.len() * (members.len() - 1) / 2;
        let cohesion = if possible > 0 {
            internal_edges as f64 / possible as f64
        } else {
            0.0
        };

        // Generate heuristic label from member names
        let label = generate_label(&members, symbol_names);

        communities.push(Community {
            id,
            label,
            members,
            cohesion,
        });
        id += 1;
    }

    // Sort by size descending
    communities.sort_by(|a, b| b.members.len().cmp(&a.members.len()));

    communities
}

/// Generate a heuristic label for a community based on common keywords in member names.
fn generate_label(
    member_uids: &[String],
    symbol_names: &HashMap<String, String>,
) -> String {
    let mut word_freq: HashMap<String, usize> = HashMap::new();

    for uid in member_uids {
        if let Some(name) = symbol_names.get(uid) {
            // Split camelCase and snake_case into words
            for word in split_identifier(name) {
                let lower = word.to_lowercase();
                if lower.len() >= 3 && !is_stop_word(&lower) {
                    *word_freq.entry(lower).or_default() += 1;
                }
            }
        }
    }

    // Pick the most common word(s)
    let mut words: Vec<_> = word_freq.into_iter().collect();
    words.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let label_words: Vec<String> = words
        .into_iter()
        .take(2)
        .map(|(w, _)| capitalize(&w))
        .collect();

    if label_words.is_empty() {
        "Unnamed".to_string()
    } else {
        label_words.join("")
    }
}

fn split_identifier(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                words.push(current.clone());
                current.clear();
            }
        } else if ch.is_uppercase() && !current.is_empty() {
            words.push(current.clone());
            current.clear();
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }

    words
}

fn is_stop_word(word: &str) -> bool {
    matches!(
        word,
        "get" | "set" | "new" | "the" | "and" | "for" | "with" | "from" | "this" | "that"
    )
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_communities_is_deterministic() {
        let edges = vec![
            ("a::foo".to_string(), "a::bar".to_string()),
            ("a::bar".to_string(), "a::baz".to_string()),
            ("a::foo".to_string(), "a::baz".to_string()),
            ("b::alpha".to_string(), "b::beta".to_string()),
            ("b::beta".to_string(), "b::gamma".to_string()),
            ("b::alpha".to_string(), "b::gamma".to_string()),
        ];
        let names: HashMap<String, String> = [
            ("a::foo", "fooHandler"),
            ("a::bar", "barService"),
            ("a::baz", "bazRepo"),
            ("b::alpha", "alphaController"),
            ("b::beta", "betaHelper"),
            ("b::gamma", "gammaUtil"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

        let run1 = detect_communities(&edges, &names);
        let run2 = detect_communities(&edges, &names);
        let run3 = detect_communities(&edges, &names);

        assert_eq!(run1.len(), run2.len());
        assert_eq!(run1.len(), run3.len());
        for i in 0..run1.len() {
            assert_eq!(run1[i].label, run2[i].label, "label mismatch at {}", i);
            assert_eq!(run1[i].label, run3[i].label, "label mismatch at {}", i);
            assert_eq!(run1[i].members, run2[i].members, "members mismatch at {}", i);
            assert_eq!(run1[i].members, run3[i].members, "members mismatch at {}", i);
        }
    }
}
