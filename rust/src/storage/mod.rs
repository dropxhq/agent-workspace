use crate::error::WsResult;
use crate::metadata::FileMetadata;
use crate::ranges::LineRange;

pub mod file;
pub mod handle;
pub mod mysql;
pub mod scoped;

pub use handle::{open_backend, open_scoped_backend, BackendHandle};

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ListReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub file_count: usize,
    pub total_size_bytes: u64,
    pub files: Vec<FileMetadata>,
}

pub trait WorkspaceBackend {
    fn read(&self, path: &str, ranges: Option<&[LineRange]>) -> WsResult<String>;

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
    ) -> WsResult<()>;

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport>;

    fn remove(&self, path: &str) -> WsResult<()>;
}
