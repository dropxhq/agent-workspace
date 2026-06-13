use std::fs;

use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::lock::FileLock;
use crate::meta::sidecar_absolute;
use crate::workspace::{is_metadata_path, parse_ws_path};

pub fn run(path: &str, config: &Config) -> WsResult<()> {
    let resolved = parse_ws_path(path, config)?;

    if is_metadata_path(&resolved.relative, &config.metadata_suffix) {
        return Err(WsError::NotFound(resolved.relative));
    }

    if !resolved.absolute.is_file() {
        return Err(WsError::NotFound(resolved.relative));
    }

    let _lock = FileLock::exclusive(&resolved.absolute)?;

    fs::remove_file(&resolved.absolute).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            WsError::NotFound(resolved.relative.clone())
        } else {
            WsError::Io(e)
        }
    })?;

    let sidecar = sidecar_absolute(config, &resolved.relative)?;
    if sidecar.exists() {
        let _ = fs::remove_file(&sidecar);
    }

    Ok(())
}
