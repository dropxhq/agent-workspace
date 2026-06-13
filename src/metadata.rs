use std::fs;
use std::path::Path;

use chrono::{DateTime, FixedOffset, Local};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::paths::{metadata_path_for, resolve_relative_in};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileMetadata {
    pub relative_path: String,
    pub created_by: String,
    pub desc: String,
    pub created_at: DateTime<FixedOffset>,
    pub updated_at: DateTime<FixedOffset>,
    pub size_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl FileMetadata {
    pub fn read_from_sidecar(path: &Path) -> WsResult<Self> {
        let contents = fs::read_to_string(path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                WsError::NotFound(path.display().to_string())
            } else {
                WsError::Io(e)
            }
        })?;
        serde_yaml::from_str(&contents)
            .map_err(|e| WsError::Other(format!("invalid metadata {}: {e}", path.display())))
    }

    pub fn write_to_sidecar(&self, path: &Path) -> WsResult<()> {
        let contents = serde_yaml::to_string(self)
            .map_err(|e| WsError::Other(format!("failed to serialize metadata: {e}")))?;
        fs::write(path, contents).map_err(WsError::Io)
    }
}

pub fn compute_sha256(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}

pub fn now_local() -> DateTime<FixedOffset> {
    Local::now().fixed_offset()
}

pub fn sidecar_absolute(config: &Config, data_relative: &str) -> WsResult<std::path::PathBuf> {
    sidecar_absolute_in(
        config.workspace_dir(),
        config.metadata_suffix(),
        data_relative,
    )
}

pub fn sidecar_absolute_in(
    workspace_dir: &Path,
    metadata_suffix: &str,
    data_relative: &str,
) -> WsResult<std::path::PathBuf> {
    let meta_relative = metadata_path_for(data_relative, metadata_suffix);
    Ok(resolve_relative_in(&meta_relative, workspace_dir)?.absolute)
}

pub fn build_metadata(
    config: &Config,
    data_relative: &str,
    content: &[u8],
    created_by: &str,
    desc: &str,
) -> WsResult<FileMetadata> {
    build_metadata_in(
        config.workspace_dir(),
        config.metadata_suffix(),
        data_relative,
        content,
        created_by,
        desc,
    )
}

pub fn build_metadata_in(
    workspace_dir: &Path,
    metadata_suffix: &str,
    data_relative: &str,
    content: &[u8],
    created_by: &str,
    desc: &str,
) -> WsResult<FileMetadata> {
    let sidecar = sidecar_absolute_in(workspace_dir, metadata_suffix, data_relative)?;
    let now = now_local();

    let (created_by_val, created_at) = if sidecar.exists() {
        match FileMetadata::read_from_sidecar(&sidecar) {
            Ok(existing) => (existing.created_by, existing.created_at),
            Err(_) => (created_by.to_string(), now),
        }
    } else {
        (created_by.to_string(), now)
    };

    Ok(FileMetadata {
        relative_path: data_relative.to_string(),
        created_by: created_by_val,
        desc: desc.to_string(),
        created_at,
        updated_at: now,
        size_bytes: content.len() as u64,
        sha256: Some(compute_sha256(content)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_value() {
        assert_eq!(
            compute_sha256(b"hello"),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
