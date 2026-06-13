pub use crate::backend::ListReport;

use crate::backend::{BackendHandle, WorkspaceBackend};
use crate::error::{WsError, WsResult};
pub fn run(path: Option<&str>, json: bool, backend: &BackendHandle) -> WsResult<()> {
    let report = backend.list(path)?;

    if json {
        let out = serde_json::to_string_pretty(&report)
            .map_err(|e| WsError::Other(format!("json serialize failed: {e}")))?;
        println!("{out}");
    } else {
        print_human(&report);
    }

    Ok(())
}

fn print_human(report: &ListReport) {
    if let Some(scope) = &report.scope {
        println!("Scope: {scope}");
    }
    println!(
        "Files: {}  Total size: {} bytes",
        report.file_count, report.total_size_bytes
    );
    println!();
    if report.files.is_empty() {
        println!("(no files)");
        return;
    }

    println!(
        "{:<40} {:<12} {:<20} {:<20} {:>10}",
        "PATH", "CREATED_BY", "CREATED_AT", "UPDATED_AT", "SIZE"
    );
    println!("{}", "-".repeat(110));

    for f in &report.files {
        println!(
            "{:<40} {:<12} {:<20} {:<20} {:>10}",
            f.relative_path,
            truncate(&f.created_by, 12),
            f.created_at.format("%Y-%m-%d %H:%M:%S"),
            f.updated_at.format("%Y-%m-%d %H:%M:%S"),
            f.size_bytes,
        );
        if !f.desc.is_empty() {
            println!("  desc: {}", f.desc);
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!(
            "{}…",
            s.chars().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}
