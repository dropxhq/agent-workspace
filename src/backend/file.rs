use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::LineRange;
use crate::error::{WsError, WsResult};
use std::path::PathBuf;

pub struct FileBackend {
    pub workspace_dir: PathBuf,
    pub metadata_suffix: String,
}

impl FileBackend {
    pub fn new(workspace_dir: PathBuf, metadata_suffix: String) -> Self {
        Self {
            workspace_dir,
            metadata_suffix,
        }
    }
}

impl WorkspaceBackend for FileBackend {
    fn read(
        &self,
        _path: &str,
        _ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String> {
        Err(WsError::Other("FileBackend::read not implemented".into()))
    }

    fn write(
        &self,
        _path: &str,
        _ranges: Option<&LineRange>,
        _content: &str,
        _created_by: &str,
        _desc: &str,
    ) -> WsResult<()> {
        Err(WsError::Other("FileBackend::write not implemented".into()))
    }

    fn list(&self, _scope: Option<&str>) -> WsResult<ListReport> {
        Err(WsError::Other("FileBackend::list not implemented".into()))
    }

    fn remove(&self, _path: &str) -> WsResult<()> {
        Err(WsError::Other("FileBackend::remove not implemented".into()))
    }
}
