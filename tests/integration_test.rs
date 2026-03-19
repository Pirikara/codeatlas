use std::fs;
use std::path::Path;
use std::process::Command;

fn codeatlas_bin() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("debug")
        .join("codeatlas");
    path.to_string_lossy().to_string()
}

fn run_codeatlas(args: &[&str]) -> (String, bool) {
    let output = Command::new(codeatlas_bin())
        .args(args)
        .output()
        .expect("Failed to execute codeatlas");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        eprintln!("STDERR: {}", stderr);
    }
    (stdout, output.status.success())
}

fn run_codeatlas_full(args: &[&str]) -> (String, String, bool) {
    let output = Command::new(codeatlas_bin())
        .args(args)
        .output()
        .expect("Failed to execute codeatlas");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

fn run_json(args: &[&str]) -> serde_json::Value {
    let (stdout, success) = run_codeatlas(args);
    assert!(success, "codeatlas failed: {}", stdout);
    serde_json::from_str(&stdout).expect("Failed to parse JSON output")
}

fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("benchmark")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn test_fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

/// Copy a fixture to a temp directory for mutation tests.
fn tempdir_copy(name: &str) -> std::path::PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("benchmark")
        .join(name);
    let dst = std::env::temp_dir().join(format!("codeatlas_test_{}_{}", name, std::process::id()));
    copy_dir_all(&src, &dst).expect("failed to copy fixture");
    dst
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        // Skip the .codeatlas directory so each test gets a clean DB
        if entry.file_name() == ".codeatlas" {
            continue;
        }
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

// ── Index + Status ──────────────────────────────────────────────

#[test]
fn index_ts_webapp() {
    let path = fixture_path("ts-webapp");
    let (_, success) = run_codeatlas(&["index", "--force", &path]);
    assert!(success);

    let status = run_json(&["status", "--json", &path]);
    assert!(status["symbol_count"].as_u64().unwrap() > 50);
    assert!(status["relationship_count"].as_u64().unwrap() > 100);
    assert_eq!(status["file_count"].as_u64().unwrap(), 12);
    assert!(status["community_count"].as_u64().unwrap() > 0);
}

#[test]
fn index_go_cli() {
    let path = fixture_path("go-cli");
    let (_, success) = run_codeatlas(&["index", "--force", &path]);
    assert!(success);

    let status = run_json(&["status", "--json", &path]);
    assert!(status["symbol_count"].as_u64().unwrap() > 25);
    assert!(status["relationship_count"].as_u64().unwrap() > 50);
    assert_eq!(status["file_count"].as_u64().unwrap(), 7);
    assert!(status["process_count"].as_u64().unwrap() > 0);
}

#[test]
fn index_ruby_service() {
    let path = fixture_path("ruby-service");
    let (_, success) = run_codeatlas(&["index", "--force", &path]);
    assert!(success);

    let status = run_json(&["status", "--json", &path]);
    assert!(status["symbol_count"].as_u64().unwrap() > 80);
    assert!(status["relationship_count"].as_u64().unwrap() > 100);
    assert_eq!(status["file_count"].as_u64().unwrap(), 10);
}

// ── Query (returns array directly) ──────────────────────────────

#[test]
fn query_finds_symbols() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["query", "UserService", "-p", &path, "--json"]);
    let results = result.as_array().expect("query returns array");
    assert!(!results.is_empty(), "query should find UserService");
    assert!(results.iter().any(|r| r["symbol"]["name"].as_str() == Some("UserService")));
}

#[test]
fn query_go_symbols() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["query", "Execute", "-p", &path, "--json"]);
    let results = result.as_array().expect("query returns array");
    assert!(results.iter().any(|r| r["symbol"]["name"].as_str() == Some("Execute")));
}

// ── Context ─────────────────────────────────────────────────────

#[test]
fn context_shows_relationships() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    let (stdout, success) = run_codeatlas(&["context", "UserService", "-p", &path, "--json"]);
    assert!(success);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // New format: status + symbol at top level
    assert_eq!(result["status"].as_str(), Some("found"));
    assert_eq!(result["symbol"]["name"].as_str(), Some("UserService"));
}

#[test]
fn context_single_match() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["context", "Execute", "-p", &path, "--json"]);
    assert_eq!(result["status"].as_str(), Some("found"), "unique name should be found");
    assert_eq!(result["symbol"]["name"].as_str(), Some("Execute"));
    assert!(result["symbol"]["uid"].as_str().is_some(), "uid field must be present");
    assert!(result["incoming"].is_array());
    assert!(result["outgoing"].is_array());
}

#[test]
fn context_ambiguous() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // findById exists in base.service.ts (BaseService) and user.repository.ts (UserRepository)
    let result = run_json(&["context", "findById", "-p", &path, "--json"]);
    assert_eq!(result["status"].as_str(), Some("ambiguous"), "duplicate name should be ambiguous");
    let candidates = result["candidates"].as_array().expect("candidates must be array");
    assert!(candidates.len() >= 2, "should have at least 2 candidates");
    assert!(result["message"].as_str().is_some(), "message must be present");
    // Each candidate must have a uid field
    for c in candidates {
        assert!(c["uid"].as_str().is_some(), "each candidate must have uid");
    }
}

#[test]
fn context_ambiguous_deterministic_order() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // Run twice and verify candidates come back in the same order
    let r1 = run_json(&["context", "findById", "-p", &path, "--json"]);
    let r2 = run_json(&["context", "findById", "-p", &path, "--json"]);
    let c1 = r1["candidates"].as_array().unwrap();
    let c2 = r2["candidates"].as_array().unwrap();
    assert_eq!(c1.len(), c2.len(), "candidate count must be deterministic");
    for (a, b) in c1.iter().zip(c2.iter()) {
        assert_eq!(a["uid"], b["uid"], "candidate order must be deterministic");
    }
    // Exported symbols should come first (is_exported DESC)
    if c1.len() >= 2 {
        let first_exported = c1[0]["is_exported"].as_bool().unwrap_or(false);
        let last_exported = c1[c1.len() - 1]["is_exported"].as_bool().unwrap_or(false);
        assert!(
            first_exported >= last_exported,
            "exported symbols should sort before unexported"
        );
    }
}

#[test]
fn context_uid_lookup() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // Get a uid from the ambiguous result
    let ambiguous = run_json(&["context", "findById", "-p", &path, "--json"]);
    assert_eq!(ambiguous["status"].as_str(), Some("ambiguous"));
    let uid = ambiguous["candidates"][0]["uid"].as_str().unwrap().to_string();

    // Use --uid for zero-ambiguity lookup
    let result = run_json(&["context", "--uid", &uid, "-p", &path, "--json"]);
    assert_eq!(result["status"].as_str(), Some("found"), "uid lookup should always be found");
    assert_eq!(result["symbol"]["uid"].as_str(), Some(uid.as_str()));
    assert_eq!(result["symbol"]["name"].as_str(), Some("findById"));
}

#[test]
fn context_file_disambiguation() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // Narrow down findById to the BaseService file
    let result = run_json(&[
        "context", "findById",
        "--file", "src/services/base.service.ts",
        "-p", &path, "--json",
    ]);
    assert_eq!(result["status"].as_str(), Some("found"), "name+file should resolve to found");
    assert_eq!(result["symbol"]["name"].as_str(), Some("findById"));
    assert!(result["symbol"]["file_path"].as_str().unwrap().contains("base.service.ts"));
}

#[test]
fn context_name_file_ambiguous() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // src/utils/dup.ts has two methods named "process" in the same class
    let result = run_json(&[
        "context", "process",
        "--file", "src/utils/dup.ts",
        "-p", &path, "--json",
    ]);
    assert_eq!(
        result["status"].as_str(), Some("ambiguous"),
        "same-file duplicate should be ambiguous, got: {}",
        result
    );
    let candidates = result["candidates"].as_array().expect("candidates must be array");
    assert_eq!(candidates.len(), 2, "should have exactly 2 candidates for 'process' in dup.ts");
}

#[test]
fn context_not_found() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["context", "NonexistentSymbol99999", "-p", &path, "--json"]);
    assert_eq!(result["status"].as_str(), Some("not_found"));
    assert!(result["message"].as_str().is_some(), "not_found must include a message");
}

// ── Impact (returns { target, direction, by_depth, total_affected }) ──

#[test]
fn impact_traces_dependencies() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    assert_eq!(result["target"]["name"].as_str(), Some("Execute"));
    assert!(result["by_depth"].is_object());
    assert!(result["total_affected"].as_u64().unwrap() > 0);
}

// ── Impact P5.2: enriched fields ─────────────────────────────────

#[test]
fn impact_new_fields_present() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    assert!(result["risk"].is_string(), "risk field must be present");
    assert!(result["summary"].is_string(), "summary field must be present");
    assert!(result["affected_modules"].is_array(), "affected_modules must be array");
    assert!(result["affected_processes"].is_array(), "affected_processes must be array");
}

#[test]
fn impact_risk_level_valid() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    let risk = result["risk"].as_str().expect("risk must be string");
    assert!(
        risk == "high" || risk == "medium" || risk == "low",
        "risk must be 'high', 'medium', or 'low', got: {}",
        risk
    );
}

#[test]
fn impact_affected_processes_nonempty_for_go_cli() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    let procs = result["affected_processes"].as_array().expect("affected_processes must be array");
    assert!(
        !procs.is_empty(),
        "Execute should affect at least one process in go-cli"
    );
    // Verify structure of each process entry
    for p in procs {
        assert!(p["id"].is_number(), "process id must be number");
        assert!(p["label"].is_string(), "process label must be string");
        assert!(p["process_type"].is_string(), "process process_type must be string");
    }
}

#[test]
fn impact_summary_message_format() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    let summary = result["summary"].as_str().expect("summary must be string");
    assert!(
        summary.starts_with("Changing Execute"),
        "summary should start with 'Changing Execute', got: {}",
        summary
    );
    let risk = result["risk"].as_str().unwrap();
    assert!(
        summary.contains(risk),
        "summary should contain the risk level '{}', got: {}",
        risk,
        summary
    );
}

// ── Clusters (returns array directly) ───────────────────────────

#[test]
fn clusters_detected() {
    let path = fixture_path("ruby-service");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["clusters", "--json", &path]);
    let clusters = result.as_array().expect("clusters returns array");
    assert!(!clusters.is_empty(), "should detect communities");
}

// ── Stale file cleanup ───────────────────────────────────────────

#[test]
fn stale_file_cleanup_on_incremental_index() {
    // Use a temp copy so we don't mutate the fixture
    let tmp = tempdir_copy("ts-webapp");
    let path = tmp.to_string_lossy().to_string();

    // Initial full index
    let (_, success) = run_codeatlas(&["index", "--force", &path]);
    assert!(success, "initial index failed");

    let before = run_json(&["status", "--json", &path]);
    let symbols_before = before["symbol_count"].as_u64().unwrap();
    assert!(symbols_before > 0);

    // Delete one source file
    let to_delete = Path::new(&path).join("src").join("utils").join("id.ts");
    fs::remove_file(&to_delete).expect("failed to delete test file");

    // Re-index incrementally (no --force)
    let (stdout, success) = run_codeatlas(&["index", &path]);
    assert!(success, "incremental index failed: {}", stdout);
    assert!(
        stdout.contains("Removed 1 deleted file(s) from index."),
        "expected cleanup message, got: {}",
        stdout
    );

    let after = run_json(&["status", "--json", &path]);
    let symbols_after = after["symbol_count"].as_u64().unwrap();
    assert!(
        symbols_after < symbols_before,
        "symbol count should decrease after file deletion (before={}, after={})",
        symbols_before,
        symbols_after
    );

    // Verify deleted file's original symbol is gone from query
    // (it may still appear as an External pseudo-symbol from unresolved calls)
    let results = run_json(&["query", "generateId", "-p", &path, "--json"]);
    let hits = results.as_array().unwrap();
    let non_external_hits: Vec<_> = hits.iter()
        .filter(|h| h["symbol"]["kind"].as_str() != Some("External"))
        .collect();
    assert!(
        non_external_hits.is_empty(),
        "stale symbol 'generateId' (non-External) should not appear after cleanup"
    );

    // Cleanup temp dir
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn incremental_index_preserves_relationships() {
    let tmp = tempdir_copy("ts-webapp");
    let path = tmp.to_string_lossy().to_string();

    // Full index
    let (_, success) = run_codeatlas(&["index", "--force", &path]);
    assert!(success, "initial index failed");
    let before = run_json(&["status", "--json", &path]);
    let rels_before = before["relationship_count"].as_u64().unwrap();
    assert!(rels_before > 50, "should have many relationships after full index");

    // Modify one file to trigger incremental re-index
    let target = Path::new(&path).join("src").join("services").join("user.service.ts");
    let mut content = fs::read_to_string(&target).unwrap();
    content.push_str("\n// incremental change\n");
    fs::write(&target, content).unwrap();

    let (_, success) = run_codeatlas(&["index", &path]);
    assert!(success, "incremental index failed");

    let after = run_json(&["status", "--json", &path]);
    let rels_after = after["relationship_count"].as_u64().unwrap();

    // Relationship count should be within a small delta (not drop to near-zero)
    let diff = (rels_before as i64 - rels_after as i64).unsigned_abs();
    assert!(
        diff <= 5,
        "incremental index should preserve relationships (before={}, after={}, diff={})",
        rels_before, rels_after, diff
    );

    fs::remove_dir_all(&tmp).ok();
}

// ── Relationship content verification ────────────────────────────

#[test]
fn relationships_have_expected_kinds() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // context should expose incoming/outgoing relationships with known kinds
    let (stdout, success) = run_codeatlas(&["context", "UserService", "-p", &path, "--json"]);
    assert!(success);
    let result: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(result["status"].as_str(), Some("found"));
    let valid_kinds = ["CALLS", "CALLS_UNRESOLVED", "CALLS_EXTERNAL", "IMPORTS", "EXTENDS", "IMPLEMENTS", "DEFINES", "CONTAINS"];

    for rel in result["incoming"].as_array().unwrap_or(&vec![]) {
        let kind = rel["kind"].as_str().unwrap();
        assert!(
            valid_kinds.contains(&kind),
            "unexpected relationship kind in incoming: {}",
            kind
        );
    }
    for rel in result["outgoing"].as_array().unwrap_or(&vec![]) {
        let kind = rel["kind"].as_str().unwrap();
        assert!(
            valid_kinds.contains(&kind),
            "unexpected relationship kind in outgoing: {}",
            kind
        );
    }
}

#[test]
fn impact_calls_only_is_subset_of_all() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let all = run_json(&["impact", "Execute", "-p", &path, "--json"]);
    let calls = run_json(&["impact", "Execute", "--calls-only", "-p", &path, "--json"]);

    let all_count = all["total_affected"].as_u64().unwrap_or(0);
    let calls_count = calls["total_affected"].as_u64().unwrap_or(0);

    assert!(
        calls_count <= all_count,
        "calls-only ({}) should be ≤ all-relationships ({})",
        calls_count,
        all_count
    );
}

// ── Eval quality gate ────────────────────────────────────────────

#[test]
fn eval_bm25_quality_gate_ts_webapp() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    let fixture_json = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("eval")
        .join("ts-webapp.json")
        .to_string_lossy()
        .to_string();

    let result = run_json(&["eval", &fixture_json, "--mode", "bm25", "-k", "5", "--json"]);

    let bm25 = &result.as_array().expect("eval returns array")[0];
    let recall = bm25["recall_at_k"].as_f64().unwrap();
    let mrr = bm25["mrr"].as_f64().unwrap();
    assert!(
        recall >= 0.4,
        "BM25 Recall@5 should be >= 0.4, got {:.3}",
        recall
    );
    assert!(
        mrr >= 0.3,
        "BM25 MRR should be >= 0.3, got {:.3}",
        mrr
    );
}

#[test]
fn eval_threshold_exits_nonzero_when_below() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    let fixture_json = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("eval")
        .join("ts-webapp.json")
        .to_string_lossy()
        .to_string();

    // 達成不可能な閾値 2.0 を指定 → exit(1) を期待
    let (_, success) = run_codeatlas(&[
        "eval",
        &fixture_json,
        "--mode",
        "bm25",
        "-k",
        "5",
        "--min-recall",
        "2.0",
    ]);
    assert!(!success, "should exit non-zero when threshold is not met");
}

// ── Eval grouped ─────────────────────────────────────────────────

#[test]
fn eval_grouped_go_cli() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let fixture_json = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata").join("eval").join("go-cli.json")
        .to_string_lossy().to_string();

    // 観点1: 基本構造と mode ラベル
    let result = run_json(&["eval", &fixture_json, "--grouped", "-k", "5", "--json"]);
    let reports = result.as_array().expect("eval returns array");
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["mode"].as_str(), Some("Grouped"));

    // 観点2: Recall@k / MRR が存在し合理的
    let recall = reports[0]["recall_at_k"].as_f64().expect("recall_at_k must be f64");
    let mrr    = reports[0]["mrr"].as_f64().expect("mrr must be f64");
    assert!(recall >= 0.4, "Grouped Recall@5 >= 0.4, got {:.3}", recall);
    assert!(mrr >= 0.3,    "Grouped MRR >= 0.3, got {:.3}", mrr);

    // 観点3: process_recall と routing_accuracy が Some かつ > 0
    let pr = reports[0]["process_recall"].as_f64()
        .expect("process_recall must be present when fixture has in_process entries");
    assert!(pr > 0.0, "process_recall > 0 for go-cli, got {:.3}", pr);
    let ra = reports[0]["routing_accuracy"].as_f64()
        .expect("routing_accuracy must be present");
    assert!(ra > 0.0, "routing_accuracy > 0 for go-cli, got {:.3}", ra);

    // 観点4: ts-webapp では process_recall / routing_accuracy が absent
    {
        let ts_path = fixture_path("ts-webapp");
        run_codeatlas(&["index", "--force", &ts_path]);
        let ts_fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata").join("eval").join("ts-webapp.json")
            .to_string_lossy().to_string();
        let ts_result = run_json(&["eval", &ts_fixture, "--grouped", "-k", "5", "--json"]);
        let ts_report = &ts_result.as_array().unwrap()[0];
        assert!(
            !ts_report.as_object().unwrap().contains_key("process_recall"),
            "ts-webapp should not have process_recall field"
        );
        assert!(
            !ts_report.as_object().unwrap().contains_key("routing_accuracy"),
            "ts-webapp should not have routing_accuracy field"
        );
    }

    // 観点5: --grouped と --mode / --all の排他（clap）
    let (_, ok) = run_codeatlas(&["eval", &fixture_json, "--grouped", "--mode", "bm25", "-k", "5"]);
    assert!(!ok, "--grouped and --mode should conflict");
    let (_, ok) = run_codeatlas(&["eval", &fixture_json, "--grouped", "--all", "-k", "5"]);
    assert!(!ok, "--grouped and --all should conflict");

    // 観点6: --min-process-hit quality gate
    let (_, ok) = run_codeatlas(&[
        "eval", &fixture_json, "--grouped", "-k", "5", "--min-process-hit", "2.0",
    ]);
    assert!(!ok, "--min-process-hit=2.0 should exit non-zero");
}

// ── Subgraph ─────────────────────────────────────────────────────

#[test]
fn subgraph_node_count_increases_with_depth() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let d1 = run_json(&["subgraph", "Execute", "-p", &path, "--depth", "1", "--json"]);
    let d2 = run_json(&["subgraph", "Execute", "-p", &path, "--depth", "2", "--json"]);
    let d3 = run_json(&["subgraph", "Execute", "-p", &path, "--depth", "3", "--json"]);

    let n1 = d1["node_count"].as_u64().unwrap_or(0);
    let n2 = d2["node_count"].as_u64().unwrap_or(0);
    let n3 = d3["node_count"].as_u64().unwrap_or(0);

    assert!(n1 > 0, "depth 1 should return at least 1 node");
    assert!(n2 >= n1, "depth 2 node count ({}) should be >= depth 1 ({})", n2, n1);
    assert!(n3 >= n2, "depth 3 node count ({}) should be >= depth 2 ({})", n3, n2);
}

#[test]
fn subgraph_truncation_on_max_nodes() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "subgraph", "Execute", "-p", &path,
        "--depth", "3", "--max-nodes", "1", "--json",
    ]);

    assert_eq!(result["truncated"].as_bool(), Some(true), "should be truncated");
    let nodes = result["nodes"].as_array().expect("nodes should be array");
    assert_eq!(nodes.len(), 1, "should have exactly 1 node");
}

#[test]
fn subgraph_edge_types_filter() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "subgraph", "Execute", "-p", &path,
        "--direction", "outgoing", "--edge-types", "CALLS", "--json",
    ]);

    let edges = result["edges"].as_array().expect("edges should be array");
    assert!(!edges.is_empty(), "Execute should have CALLS edges");
    for edge in edges {
        assert_eq!(
            edge["kind"].as_str(),
            Some("CALLS"),
            "all edges should be CALLS kind, got: {}",
            edge["kind"]
        );
    }
}

#[test]
fn subgraph_terminates_on_deep_traversal() {
    // BFS termination on a large real-world fixture at high depth.
    // Cycle safety is proven at the unit level (query::tests::subgraph_terminates_on_cycle),
    // which uses an explicit A→B→C→A graph to confirm the HashSet visited guard works.
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "subgraph", "Execute", "-p", &path,
        "--depth", "10", "--direction", "both", "--json",
    ]);

    assert!(result["node_count"].as_u64().unwrap_or(0) > 0,
        "BFS should return nodes and terminate without timeout at depth=10");
}

#[test]
fn subgraph_terminates_on_cyclic_fixture() {
    // go-cycle fixture contains explicit mutual-call cycles (Ping↔Pong, Relay↔Forward).
    // This integration test validates that the BFS visited guard works end-to-end
    // through the full CLI path on a graph that is known to have cycles.
    let path = fixture_path("go-cycle");
    run_codeatlas(&["index", "--force", &path]);

    // Verify the fixture actually contains mutual cycles before testing BFS on it.
    let cycle_check = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM relationships r1 \
         JOIN relationships r2 ON r1.source_id = r2.target_id AND r1.target_id = r2.source_id \
         WHERE r1.kind = 'CALLS'",
        "-p", &path, "--json",
    ]);
    let cycle_count = cycle_check[0]["cnt"].as_u64().unwrap_or(0);
    assert!(cycle_count > 0, "go-cycle fixture must have mutual CALLS cycles (Ping↔Pong)");

    // Run subgraph with high depth; if BFS doesn't terminate, the test will time out.
    let result = run_json(&[
        "subgraph", "Ping", "-p", &path,
        "--depth", "20", "--direction", "both", "--json",
    ]);

    // All symbols reachable from Ping in a cycle graph should be found.
    assert!(result["node_count"].as_u64().unwrap_or(0) >= 2,
        "should reach at least Ping and Pong despite cycle");
    assert_eq!(result["truncated"].as_bool(), Some(false),
        "small fixture should not be truncated");
}

#[test]
fn subgraph_by_id() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    // Get Execute's id via graph-query
    let rows = run_json(&[
        "graph-query",
        "SELECT id, uid FROM symbols WHERE name='Execute' LIMIT 1",
        "-p", &path, "--json",
    ]);
    let id = rows[0]["id"].as_i64().expect("id should be integer");

    let result = run_json(&[
        "subgraph", "--id", &id.to_string(), "-p", &path, "--json",
    ]);
    assert!(result["node_count"].as_u64().unwrap_or(0) > 0, "subgraph --id should return nodes");
    assert_eq!(result["start_id"].as_i64(), Some(id), "start_id should match given --id");
}

#[test]
fn subgraph_by_uid() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    // Get Execute's uid
    let rows = run_json(&[
        "graph-query",
        "SELECT id, uid FROM symbols WHERE name='Execute' LIMIT 1",
        "-p", &path, "--json",
    ]);
    let id = rows[0]["id"].as_i64().expect("id should be integer");
    let uid = rows[0]["uid"].as_str().expect("uid should be string").to_string();

    let result = run_json(&[
        "subgraph", "--uid", &uid, "-p", &path, "--json",
    ]);
    assert!(result["node_count"].as_u64().unwrap_or(0) > 0, "subgraph --uid should return nodes");
    assert_eq!(result["start_id"].as_i64(), Some(id), "start_id should match Execute's id");
}

// ── Impact batch (P4: VCS-independent) ───────────────────────────

#[test]
fn impact_batch_by_ranges() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    // Query the range covering Execute in cmd/root.go (lines 1–30 covers the function)
    let ranges = r#"[{"file":"cmd/root.go","start":1,"end":30}]"#;
    let result = run_json(&["impact-batch", "-p", &path, "--ranges", ranges, "--json"]);

    let results = result["results"].as_array().unwrap();
    assert!(!results.is_empty(), "should find symbols in range");
    assert!(
        results.iter().any(|r| r["symbol"]["name"].as_str() == Some("Execute")),
        "Execute should be among results"
    );

    // All results must be Function or Method (default kind filter)
    for r in results {
        let kind = r["symbol"]["kind"].as_str().unwrap();
        assert!(
            kind == "Function" || kind == "Method",
            "default filter should only return Function/Method, got: {}",
            kind
        );
    }

    // total and truncated fields must be present
    assert!(result["total"].as_u64().unwrap() > 0);
    assert!(result["truncated"].is_boolean());
}

#[test]
fn impact_batch_by_name_file() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let symbols = r#"[{"name":"Execute","file":"cmd/root.go"}]"#;
    let result = run_json(&["impact-batch", "-p", &path, "--symbols", symbols, "--json"]);

    let results = result["results"].as_array().unwrap();
    assert!(!results.is_empty(), "should find Execute by name+file");
    assert_eq!(results[0]["symbol"]["name"].as_str(), Some("Execute"));
}

#[test]
fn impact_batch_all_kinds() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let ranges = r#"[{"file":"cmd/root.go","start":1,"end":30}]"#;

    // Default (Function/Method only)
    let default_result = run_json(&["impact-batch", "-p", &path, "--ranges", ranges, "--json"]);
    let default_count = default_result["results"].as_array().unwrap().len();

    // --all-kinds must return >= default count
    let all_result = run_json(&["impact-batch", "-p", &path, "--ranges", ranges, "--all-kinds", "--json"]);
    let all_count = all_result["results"].as_array().unwrap().len();
    assert!(
        all_count >= default_count,
        "--all-kinds ({}) should be >= default ({})",
        all_count,
        default_count
    );
}

#[test]
fn impact_batch_invalid_input() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    // Neither --symbols nor --ranges → exit non-zero
    let (_, ok) = run_codeatlas(&["impact-batch", "-p", &path]);
    assert!(!ok, "missing --symbols and --ranges should exit non-zero");

    // Invalid JSON → exit non-zero
    let (_, ok) = run_codeatlas(&["impact-batch", "-p", &path, "--ranges", "not-json"]);
    assert!(!ok, "invalid JSON should exit non-zero");

    // Invalid kind → exit non-zero
    let ranges = r#"[{"file":"cmd/root.go","start":1,"end":30}]"#;
    let (_, ok) = run_codeatlas(&["impact-batch", "-p", &path, "--ranges", ranges, "--kinds", "BadKind"]);
    assert!(!ok, "invalid kind should exit non-zero");

    // --all-kinds and --kinds together → clap conflict → exit non-zero
    let (_, ok) = run_codeatlas(&["impact-batch", "-p", &path, "--ranges", ranges, "--all-kinds", "--kinds", "Function"]);
    assert!(!ok, "--all-kinds + --kinds should exit non-zero");
}

// ── Registry (P5.5) ──────────────────────────────────────────────

#[test]
fn registry_created_after_index() {
    let dir = tempdir_copy("go-cli");
    let registry_path = std::env::temp_dir().join(format!(
        "codeatlas_registry_test_{}.json",
        std::process::id()
    ));
    // Ensure clean state
    let _ = std::fs::remove_file(&registry_path);

    let output = std::process::Command::new(codeatlas_bin())
        .args(["index", "--force", &dir.to_string_lossy()])
        .env("CODEATLAS_REGISTRY_PATH", &registry_path)
        .output()
        .expect("Failed to execute codeatlas");
    assert!(output.status.success(), "index should succeed");

    assert!(registry_path.exists(), "registry.json should be created after index");

    let text = std::fs::read_to_string(&registry_path).unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(entries.len(), 1, "should have exactly 1 entry");
    assert!(entries[0]["name"].is_string(), "entry should have name");
    assert!(entries[0]["path"].is_string(), "entry should have path");
    assert!(entries[0]["indexed_at"].is_string(), "entry should have indexed_at");

    let _ = std::fs::remove_file(&registry_path);
}

#[test]
fn registry_reindex_preserves_name_updates_indexed_at() {
    let dir = tempdir_copy("go-cli");
    let registry_path = std::env::temp_dir().join(format!(
        "codeatlas_registry_reindex_{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&registry_path);

    // First index
    std::process::Command::new(codeatlas_bin())
        .args(["index", "--force", &dir.to_string_lossy()])
        .env("CODEATLAS_REGISTRY_PATH", &registry_path)
        .output()
        .expect("Failed to execute codeatlas");

    let text1 = std::fs::read_to_string(&registry_path).unwrap();
    let entries1: Vec<serde_json::Value> = serde_json::from_str(&text1).unwrap();
    let name1 = entries1[0]["name"].as_str().unwrap().to_string();
    let ts1 = entries1[0]["indexed_at"].as_str().unwrap().to_string();

    // Brief sleep to ensure timestamp differs
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Second index (re-index)
    std::process::Command::new(codeatlas_bin())
        .args(["index", "--force", &dir.to_string_lossy()])
        .env("CODEATLAS_REGISTRY_PATH", &registry_path)
        .output()
        .expect("Failed to execute codeatlas");

    let text2 = std::fs::read_to_string(&registry_path).unwrap();
    let entries2: Vec<serde_json::Value> = serde_json::from_str(&text2).unwrap();
    assert_eq!(entries2.len(), 1, "should still have 1 entry after re-index");
    let name2 = entries2[0]["name"].as_str().unwrap().to_string();
    let ts2 = entries2[0]["indexed_at"].as_str().unwrap().to_string();

    assert_eq!(name1, name2, "name should not change on re-index");
    assert_ne!(ts1, ts2, "indexed_at should be updated on re-index");

    let _ = std::fs::remove_file(&registry_path);
}

#[test]
fn registry_name_conflict_gets_suffix() {
    // Both repos must share the same basename ("go-cli") but live at different paths.
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("testdata").join("benchmark").join("go-cli");

    let base1 = std::env::temp_dir()
        .join(format!("codeatlas_conflict_a_{}", std::process::id()));
    let dir1 = base1.join("go-cli");
    copy_dir_all(&src, &dir1).expect("failed to copy dir1");

    let base2 = std::env::temp_dir()
        .join(format!("codeatlas_conflict_b_{}", std::process::id()));
    let dir2 = base2.join("go-cli");
    copy_dir_all(&src, &dir2).expect("failed to copy dir2");

    let registry_path = std::env::temp_dir().join(format!(
        "codeatlas_registry_conflict_{}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&registry_path);

    // Index first repo
    std::process::Command::new(codeatlas_bin())
        .args(["index", "--force", &dir1.to_string_lossy()])
        .env("CODEATLAS_REGISTRY_PATH", &registry_path)
        .output()
        .expect("Failed to execute codeatlas");

    // Index second repo (different path, same basename)
    std::process::Command::new(codeatlas_bin())
        .args(["index", "--force", &dir2.to_string_lossy()])
        .env("CODEATLAS_REGISTRY_PATH", &registry_path)
        .output()
        .expect("Failed to execute codeatlas");

    let text = std::fs::read_to_string(&registry_path).unwrap();
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert_eq!(entries.len(), 2, "should have 2 entries for 2 different paths");

    let names: Vec<&str> = entries.iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"go-cli"), "first entry should be 'go-cli'");
    assert!(names.contains(&"go-cli-2"), "second entry should be 'go-cli-2'");

    let _ = std::fs::remove_file(&registry_path);
    let _ = std::fs::remove_dir_all(&base1);
    let _ = std::fs::remove_dir_all(&base2);
}

// ── Processes (returns array directly) ──────────────────────────

#[test]
fn processes_detected() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["processes", "--json", &path]);
    let processes = result.as_array().expect("processes returns array");
    assert!(!processes.is_empty(), "should detect execution flows");
    // Verify process structure
    let first = &processes[0];
    assert!(first["label"].is_string());
    assert!(first["steps"].is_array());
    assert!(first["steps"].as_array().unwrap().len() >= 2);
}

// ── Query grouped (P5.3) ─────────────────────────────────────────

#[test]
fn query_grouped_returns_structured_result() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["query", "Execute", "-p", &path, "--grouped", "--json"]);

    // Must have processes, definitions, total fields
    assert!(result["processes"].is_array(), "grouped result must have 'processes' array");
    assert!(result["definitions"].is_array(), "grouped result must have 'definitions' array");
    let total = result["total"].as_u64().expect("grouped result must have 'total' as integer");
    assert!(total > 0, "total should be > 0 for a known symbol");
}

#[test]
fn query_grouped_processes_nonempty_for_go_cli() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&["query", "Execute", "-p", &path, "--grouped", "--json"]);

    let processes = result["processes"].as_array().expect("processes should be array");
    assert!(!processes.is_empty(), "Execute should belong to at least one process in go-cli");

    // Validate process structure
    let first = &processes[0];
    assert!(first["id"].is_number(), "process must have id");
    assert!(first["label"].is_string(), "process must have label");
    assert!(first["process_type"].is_string(), "process must have process_type");
    let matched = first["matched_symbols"].as_array().expect("matched_symbols should be array");
    assert!(!matched.is_empty(), "matched_symbols should not be empty");
    assert!(matched[0]["step_index"].is_number(), "matched_symbol must have step_index");
    assert!(matched[0]["score"].is_number(), "matched_symbol must have score");
}

#[test]
fn query_grouped_definitions_for_symbol_outside_process() {
    let path = fixture_path("ts-webapp");
    run_codeatlas(&["index", "--force", &path]);

    // UserService should appear in grouped results (either definitions or processes)
    let result = run_json(&["query", "UserService", "-p", &path, "--grouped", "--json"]);

    let definitions = result["definitions"].as_array().expect("definitions should be array");
    let processes = result["processes"].as_array().expect("processes should be array");
    let total = result["total"].as_u64().unwrap_or(0);
    assert!(
        !definitions.is_empty() || !processes.is_empty(),
        "UserService should appear in grouped results (definitions={}, processes={}, total={})",
        definitions.len(), processes.len(), total
    );
}

#[test]
fn query_grouped_rejects_mode_flag() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    // --grouped and --mode are mutually exclusive via clap conflicts_with
    let (_, ok) = run_codeatlas(&["query", "Execute", "-p", &path, "--grouped", "--mode", "hybrid"]);
    assert!(!ok, "--grouped and --mode should conflict and exit non-zero");
}

// ── Graph query (P5.4) ────────────────────────────────────────────

#[test]
fn graph_query_select_returns_rows() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "graph-query",
        "SELECT name, kind FROM symbols WHERE kind='Function' LIMIT 5",
        "-p", &path, "--json",
    ]);
    let rows = result.as_array().expect("graph-query returns array");
    assert!(!rows.is_empty(), "should return rows");
    assert!(rows[0]["name"].is_string(), "rows should have name field");
}

#[test]
fn graph_query_with_cte_is_allowed() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "graph-query",
        "WITH funcs AS (SELECT name FROM symbols WHERE kind='Function') SELECT name FROM funcs LIMIT 3",
        "-p", &path, "--json",
    ]);
    let rows = result.as_array().expect("CTE query returns array");
    assert!(!rows.is_empty(), "WITH...SELECT should return rows");
}

#[test]
fn graph_query_limit_is_respected() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let result = run_json(&[
        "graph-query",
        "SELECT id FROM symbols",
        "-p", &path, "--limit", "3", "--json",
    ]);
    let rows = result.as_array().expect("returns array");
    assert!(rows.len() <= 3, "limit=3 should return at most 3 rows, got {}", rows.len());
}

#[test]
fn graph_query_rejects_insert() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let (_, ok) = run_codeatlas(&[
        "graph-query",
        "INSERT INTO symbols(uid,name,kind,file_path) VALUES('x','y','z','w')",
        "-p", &path,
    ]);
    assert!(!ok, "INSERT should be rejected with non-zero exit");
}

#[test]
fn graph_query_rejects_drop() {
    let path = fixture_path("go-cli");
    run_codeatlas(&["index", "--force", &path]);

    let (_, ok) = run_codeatlas(&[
        "graph-query",
        "DROP TABLE symbols",
        "-p", &path,
    ]);
    assert!(!ok, "DROP TABLE should be rejected with non-zero exit");
}

// ── P6: Analysis accuracy ─────────────────────────────────────────

#[test]
fn ruby_send_creates_calls_edges() {
    let path = fixture_path("ruby-dynamic");
    run_codeatlas(&["index", "--force", &path]);

    // Dispatcher.run should CALLS Dispatcher.process (via send(:process))
    let process_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s1.parent_name, s2.name, s2.parent_name \
         FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'run' AND s1.parent_name = 'Dispatcher' \
           AND s2.name = 'process' AND s2.parent_name = 'Dispatcher'",
        "-p", &path, "--json",
    ]);
    assert!(
        !process_hits.as_array().unwrap().is_empty(),
        "send(:process) should create CALLS edge from Dispatcher.run to Dispatcher.process"
    );

    // Dispatcher.run should CALLS Dispatcher.notify (via public_send(:notify))
    let notify_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s1.parent_name, s2.name, s2.parent_name \
         FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'run' AND s1.parent_name = 'Dispatcher' \
           AND s2.name = 'notify' AND s2.parent_name = 'Dispatcher'",
        "-p", &path, "--json",
    ]);
    assert!(
        !notify_hits.as_array().unwrap().is_empty(),
        "public_send(:notify) should create CALLS edge from run to notify"
    );

    // There should be no CALLS edge with target name 'send' (send itself should not be emitted)
    let send_hits = run_json(&[
        "graph-query",
        "SELECT s2.name FROM relationships r \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' AND s2.name = 'send'",
        "-p", &path, "--json",
    ]);
    assert!(
        send_hits.as_array().unwrap().is_empty(),
        "send() itself should not produce a CALLS edge with target 'send'"
    );
}

#[test]
fn go_interface_impl_requires_param_count_match() {
    let path = fixture_path("go-iface-sig");
    run_codeatlas(&["index", "--force", &path]);

    // CorrectRunner IMPLEMENTS Runner (signature fully matches)
    let correct_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s2.name FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'IMPLEMENTS' AND s1.name = 'CorrectRunner' AND s2.name = 'Runner'",
        "-p", &path, "--json",
    ]);
    assert_eq!(
        correct_hits.as_array().unwrap().len(),
        1,
        "CorrectRunner should IMPLEMENTS Runner (signature match)"
    );

    // WrongRunner should NOT IMPLEMENTS Runner (Run() vs Run(x int))
    let wrong_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s2.name FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'IMPLEMENTS' AND s1.name = 'WrongRunner' AND s2.name = 'Runner'",
        "-p", &path, "--json",
    ]);
    assert!(
        wrong_hits.as_array().unwrap().is_empty(),
        "WrongRunner should NOT IMPLEMENTS Runner (param count mismatch: Run() vs Run(x int))"
    );
}

#[test]
fn ts_constructor_field_type_resolves_calls() {
    let path = fixture_path("ts-di");
    run_codeatlas(&["index", "--force", &path]);

    // Controller.create should CALLS UserService.save (via this.service.save())
    let hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s1.parent_name, s2.name, s2.parent_name, s2.file_path \
         FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'create' AND s1.parent_name = 'Controller' \
           AND s2.name = 'save'   AND s2.parent_name = 'UserService'",
        "-p", &path, "--json",
    ]);
    assert!(
        !hits.as_array().unwrap().is_empty(),
        "this.service.save() should resolve to UserService.save via constructor type annotation"
    );
}

#[test]
fn ruby_method_missing_fallback_creates_calls_edge() {
    let path = fixture_path("ruby-method-missing");
    run_codeatlas(&["index", "--force", &path]);

    // (1) wrap → method_missing CALLS edge exists (find_user and update_record both route here,
    //     but the (source_id, target_id, kind) unique constraint collapses them to 1 row)
    let mm_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s2.name, r.confidence FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'wrap' AND s1.parent_name = 'DynamicProxy' \
           AND s2.name = 'method_missing' AND s2.parent_name = 'DynamicProxy'",
        "-p", &path, "--json",
    ]);
    assert_eq!(
        mm_hits.as_array().unwrap().len(),
        1,
        "wrap should have exactly 1 CALLS edge to method_missing (deduped by unique constraint)"
    );
    // Verify low confidence (0.30)
    let confidence = mm_hits[0]["confidence"].as_f64().unwrap_or(0.0);
    assert!(
        (confidence - 0.30).abs() < 0.01,
        "method-missing-fallback confidence should be 0.30, got {}",
        confidence
    );

    // (2) wrap → known_helper CALLS edge exists (resolved normally via Strategy 2)
    let kh_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s2.name FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'wrap' AND s1.parent_name = 'DynamicProxy' \
           AND s2.name = 'known_helper' AND s2.parent_name = 'DynamicProxy'",
        "-p", &path, "--json",
    ]);
    assert!(
        !kh_hits.as_array().unwrap().is_empty(),
        "wrap should CALLS known_helper (Strategy 2 same-file resolution)"
    );

    // (3) known_helper → method_missing should NOT exist (no unresolved calls inside known_helper)
    let no_hits = run_json(&[
        "graph-query",
        "SELECT s1.name, s2.name FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         WHERE r.kind = 'CALLS' \
           AND s1.name = 'known_helper' AND s2.name = 'method_missing'",
        "-p", &path, "--json",
    ]);
    assert!(
        no_hits.as_array().unwrap().is_empty(),
        "known_helper should NOT have a CALLS edge to method_missing"
    );
}

// ── P8: Deterministic output ────────────────────────────────────

#[test]
fn deterministic_index_produces_identical_output() {
    let tmp = tempdir_copy("ts-webapp");
    let path = tmp.to_string_lossy().to_string();

    // Queries using business keys (not auto-increment ids) for regression resilience
    let community_query = "SELECT label, symbol_count FROM communities ORDER BY label, symbol_count";
    let process_query = "SELECT label, process_type, priority FROM processes ORDER BY label";
    let relationship_query =
        "SELECT s1.file_path || ':' || s1.name AS src, \
                s2.file_path || ':' || s2.name AS tgt, \
                r.kind, r.confidence \
         FROM relationships r \
         JOIN symbols s1 ON r.source_id = s1.id \
         JOIN symbols s2 ON r.target_id = s2.id \
         ORDER BY src, tgt, r.kind";

    // Run 1
    run_codeatlas(&["index", "--force", &path]);
    let comm1 = run_json(&["graph-query", community_query, "-p", &path, "--json"]);
    let proc1 = run_json(&["graph-query", process_query, "-p", &path, "--json"]);
    let rel1 = run_json(&["graph-query", relationship_query, "-p", &path, "--json"]);

    // Run 2
    run_codeatlas(&["index", "--force", &path]);
    let comm2 = run_json(&["graph-query", community_query, "-p", &path, "--json"]);
    let proc2 = run_json(&["graph-query", process_query, "-p", &path, "--json"]);
    let rel2 = run_json(&["graph-query", relationship_query, "-p", &path, "--json"]);

    assert_eq!(comm1, comm2, "Communities should be identical across runs");
    assert_eq!(proc1, proc2, "Processes should be identical across runs");
    assert_eq!(rel1, rel2, "Relationships should be identical across runs");

    // Cleanup
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn index_metrics_flag_outputs_timing() {
    let path = fixture_path("ts-webapp");
    let (_, stderr, ok) = run_codeatlas_full(&["index", "--force", "--metrics", &path]);
    assert!(ok, "index --metrics should succeed");
    assert!(stderr.contains("parse_duration"), "metrics should include parse_duration");
    assert!(stderr.contains("symbol_count"), "metrics should include symbol_count");
    assert!(stderr.contains("parse_failures"), "metrics should include parse_failures");
    assert!(stderr.contains("peak_rss_bytes"), "metrics should include peak_rss_bytes");
}

// ── P9.1: CALLS_UNRESOLVED / CALLS_EXTERNAL ─────────────────────

#[test]
fn calls_unresolved_appears_in_context() {
    let path = test_fixture_path("external-calls");
    run_codeatlas(&["index", "--force", &path]);

    // handleRequest calls JSON.parse and console.log which are unresolved
    let json = run_json(&["context", "handleRequest", "-p", &path, "--json"]);
    assert_eq!(json["status"].as_str(), Some("found"));

    let outgoing = json["outgoing"].as_array().expect("outgoing should be array");
    let unresolved_kinds: Vec<&str> = outgoing.iter()
        .filter_map(|r| r["kind"].as_str())
        .filter(|k| *k == "CALLS_UNRESOLVED" || *k == "CALLS_EXTERNAL")
        .collect();
    assert!(
        !unresolved_kinds.is_empty(),
        "handleRequest should have CALLS_UNRESOLVED/EXTERNAL outgoing edges, got: {:?}",
        outgoing
    );
}

#[test]
fn external_symbols_exist_in_graph() {
    let path = test_fixture_path("external-calls");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&[
        "graph-query",
        "SELECT name FROM symbols WHERE kind='External' ORDER BY name",
        "-p", &path, "--json",
    ]);
    let rows = json.as_array().expect("graph-query result should be array");
    assert!(
        !rows.is_empty(),
        "External symbols should exist in the graph after indexing"
    );
}

#[test]
fn external_symbols_cleaned_on_reindex() {
    let path = test_fixture_path("external-calls");
    run_codeatlas(&["index", "--force", &path]);

    let count1 = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM symbols WHERE kind='External'",
        "-p", &path, "--json",
    ]);

    // Re-index
    run_codeatlas(&["index", "--force", &path]);

    let count2 = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM symbols WHERE kind='External'",
        "-p", &path, "--json",
    ]);

    assert_eq!(
        count1, count2,
        "External symbol count should be identical across re-indexes"
    );
}

#[test]
fn calls_external_only_for_namespace() {
    let path = test_fixture_path("external-calls");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&[
        "graph-query",
        "SELECT r.kind, s2.name as target_name FROM relationships r JOIN symbols s2 ON r.target_id=s2.id WHERE r.kind IN ('CALLS_UNRESOLVED','CALLS_EXTERNAL') ORDER BY r.kind, s2.name",
        "-p", &path, "--json",
    ]);
    let rows = json.as_array().expect("graph-query result should be array");

    for row in rows {
        let kind = row["kind"].as_str().unwrap_or("");
        let target = row["target_name"].as_str().unwrap_or("");
        if kind == "CALLS_EXTERNAL" {
            // CALLS_EXTERNAL targets should have "::" in their name (namespace separator)
            assert!(
                target.contains("::"),
                "CALLS_EXTERNAL target '{}' should contain '::'",
                target
            );
        }
    }

    // Verify we have at least one CALLS_EXTERNAL (from ActiveRecord::Base)
    let has_external = rows.iter().any(|r| r["kind"].as_str() == Some("CALLS_EXTERNAL"));
    assert!(has_external, "Should have at least one CALLS_EXTERNAL relationship from Ruby namespace calls");
}

// ── P9.2: DATA_FLOWS_TO ─────────────────────────────────────────

#[test]
fn dataflow_returns_flows_for_ts_function() {
    let path = test_fixture_path("dataflow");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&["dataflow", "handleRequest", "-p", &path, "--json"]);
    assert_eq!(json["symbol"]["name"].as_str(), Some("handleRequest"));

    let flows = json["flows"].as_array().expect("flows should be array");
    assert!(!flows.is_empty(), "handleRequest should have data flows");

    // Check we have various flow kinds
    let kinds: Vec<&str> = flows.iter()
        .filter_map(|f| f["flow_kind"].as_str())
        .collect();
    assert!(kinds.contains(&"Assignment"), "should have Assignment flow");
    assert!(kinds.contains(&"StringInterp"), "should have StringInterp flow");
    assert!(kinds.contains(&"Return"), "should have Return flow");
}

#[test]
fn dataflow_ruby_flows_detected() {
    let path = test_fixture_path("dataflow");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&["dataflow", "process", "--file", "src/processor.rb", "-p", &path, "--json"]);
    assert_eq!(json["symbol"]["name"].as_str(), Some("process"));

    let flows = json["flows"].as_array().expect("flows should be array");
    assert!(!flows.is_empty(), "process() should have data flows");

    let kinds: Vec<&str> = flows.iter()
        .filter_map(|f| f["flow_kind"].as_str())
        .collect();
    assert!(kinds.contains(&"Assignment"), "should have Assignment flow");
    assert!(kinds.contains(&"Return"), "should have Return flow");
}

#[test]
fn dataflow_go_flows_detected() {
    let path = test_fixture_path("dataflow");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&["dataflow", "serve", "-p", &path, "--json"]);
    assert_eq!(json["symbol"]["name"].as_str(), Some("serve"));

    let flows = json["flows"].as_array().expect("flows should be array");
    assert!(!flows.is_empty(), "serve() should have data flows");

    let kinds: Vec<&str> = flows.iter()
        .filter_map(|f| f["flow_kind"].as_str())
        .collect();
    assert!(kinds.contains(&"Assignment"), "should have Assignment flow");
}

#[test]
fn dataflow_table_populated_after_index() {
    let path = test_fixture_path("dataflow");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM data_flows",
        "-p", &path, "--json",
    ]);
    let rows = json.as_array().expect("graph-query result should be array");
    let count = rows[0]["cnt"].as_i64().unwrap_or(0);
    assert!(count > 0, "data_flows table should have entries after indexing");
}

// ── Search Quality ────────────────────────────────────────────────

#[test]
fn query_excludes_file_and_folder_nodes() {
    let path = test_fixture_path("external-calls");
    run_codeatlas(&["index", "--force", &path]);

    let json = run_json(&["query", "main", "-p", &path, "--json"]);
    let results = json.as_array().expect("query results should be a JSON array");

    for result in results {
        let kind = result["symbol"]["kind"].as_str().unwrap_or("");
        assert!(
            kind != "File" && kind != "Folder" && kind != "External",
            "query results should not include File/Folder/External nodes, got kind='{}'",
            kind
        );
    }
}

#[test]
fn exclude_tests_flag_skips_spec_files() {
    let path = test_fixture_path("external-calls");

    // Without --exclude-tests, count all files
    run_codeatlas(&["index", "--force", &path]);

    let without = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM file_index",
        "-p", &path, "--json",
    ]);
    let cnt_without = without.as_array().unwrap()[0]["cnt"].as_i64().unwrap_or(0);

    // --exclude-tests: fixture has no spec files so count should be same or fewer
    run_codeatlas(&["index", "--force", "--exclude-tests", &path]);
    let with_excl = run_json(&[
        "graph-query",
        "SELECT COUNT(*) as cnt FROM file_index",
        "-p", &path, "--json",
    ]);
    let cnt_with = with_excl.as_array().unwrap()[0]["cnt"].as_i64().unwrap_or(0);

    assert!(
        cnt_with <= cnt_without,
        "--exclude-tests should not increase file count ({} vs {})",
        cnt_with, cnt_without
    );
}
