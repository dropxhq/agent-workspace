use std::path::PathBuf;

use walkdir::WalkDir;

use serde::Serialize;

use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::meta::FileMetadata;
use crate::workspace::{data_path_from_metadata, is_metadata_path, parse_ws_path_for_write};

#[derive(Debug, Serialize)]
pub struct ListReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub files: Vec<FileMetadata>,
}

pub fn run(path: Option<&str>, json: bool, config: &Config) -> WsResult<()> {
    let report = build_report(path, config)?;

    if json {
        let out = serde_json::to_string_pretty(&report)
            .map_err(|e| WsError::Other(format!("json serialize failed: {e}")))?;
        println!("{out}");
    } else {
        print_human(&report);
    }

    Ok(())
}

pub fn build_report(path: Option<&str>, config: &Config) -> WsResult<ListReport> {
    let (scan_root, scope) = resolve_list_scope(path, config)?;

    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    for entry in WalkDir::new(&scan_root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !file_name.ends_with(config.metadata_suffix()) {
            continue;
        }

        let rel = path
            .strip_prefix(config.workspace_dir())
            .map_err(|e| WsError::Other(e.to_string()))?;
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        if data_path_from_metadata(&rel_str, config.metadata_suffix()).is_none() {
            continue;
        }

        match FileMetadata::read_from_sidecar(path) {
            Ok(meta) => {
                if !matches_scope(&meta.relative_path, &scope) {
                    continue;
                }
                total_size += meta.size_bytes;
                files.push(meta);
            }
            Err(_) => continue,
        }
    }

    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    Ok(ListReport {
        scope,
        file_count: files.len(),
        total_size_bytes: total_size,
        files,
    })
}

fn resolve_list_scope(path: Option<&str>, config: &Config) -> WsResult<(PathBuf, Option<String>)> {
    let Some(path) = path else {
        return Ok((config.workspace_dir().clone(), None));
    };

    let resolved = parse_ws_path_for_write(path, config)?;

    if is_metadata_path(&resolved.relative, config.metadata_suffix()) {
        return Err(WsError::NotFound(resolved.relative));
    }

    let prefix = resolved.relative;

    if prefix.is_empty() {
        return Ok((config.workspace_dir().clone(), None));
    }

    let dir = resolved.absolute;
    if !dir.is_dir() {
        return Err(WsError::NotFound(prefix));
    }

    Ok((dir, Some(prefix)))
}

fn matches_scope(relative_path: &str, scope: &Option<String>) -> bool {
    match scope {
        None => true,
        Some(prefix) => relative_path == prefix.as_str() || relative_path.starts_with(&format!("{prefix}/")),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_scope_prefix() {
        assert!(matches_scope("docs/foo.txt", &Some("docs".to_string())));
        assert!(matches_scope("docs", &Some("docs".to_string())));
        assert!(!matches_scope("docs-extra/foo.txt", &Some("docs".to_string())));
        assert!(matches_scope("anything", &None));
    }
}
