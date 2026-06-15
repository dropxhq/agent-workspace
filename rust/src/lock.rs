use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use fs4::FileExt;

use crate::error::{WsError, WsResult};

pub struct FileLock {
    #[allow(dead_code)]
    file: File,
}

impl FileLock {
    pub fn shared(path: &Path) -> WsResult<Self> {
        let file = open_for_lock(path)?;
        FileExt::lock_shared(&file).map_err(lock_error)?;
        Ok(Self { file })
    }

    pub fn exclusive(path: &Path) -> WsResult<Self> {
        let file = open_for_lock(path)?;
        FileExt::lock_exclusive(&file).map_err(lock_error)?;
        Ok(Self { file })
    }
}

fn open_for_lock(path: &Path) -> io::Result<File> {
    if path.exists() {
        OpenOptions::new().read(true).write(true).open(path)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
    }
}

fn lock_error(e: io::Error) -> WsError {
    if e.kind() == io::ErrorKind::WouldBlock {
        WsError::LockConflict(e.to_string())
    } else {
        WsError::Io(e)
    }
}
