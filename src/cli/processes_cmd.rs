use crate::cli::ProcessesArgs;
use crate::storage::Database;
use anyhow::Result;

pub fn run(args: ProcessesArgs, json: bool) -> Result<()> {
    let path = args.path.canonicalize()?;
    let db_path = path.join(".codeatlas").join("index.db");
    if !db_path.exists() {
        anyhow::bail!("No index found. Run `codeatlas index` first.");
    }

    let db = Database::open(&db_path)?;
    let processes = db.list_processes()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&processes)?);
        return Ok(());
    }

    if processes.is_empty() {
        println!("No execution flows detected.");
        return Ok(());
    }

    println!("Execution flows ({}):\n", processes.len());
    for proc in &processes {
        println!(
            "  #{}: {} [{}] — {} steps, priority: {:.2}",
            proc.id, proc.label, proc.process_type, proc.step_count, proc.priority
        );
        for step in &proc.steps {
            println!(
                "    {}. {} {} ({})",
                step.step_index + 1,
                step.kind,
                step.name,
                step.file_path
            );
        }
        println!();
    }

    Ok(())
}
