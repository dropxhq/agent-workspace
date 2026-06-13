use crate::commands::ranges::LineRange;
use crate::error::WsResult;
use crate::meta::FileMetadata;

pub mod file;
pub mod mysql;
pub mod path;

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
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String>;

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

pub enum BackendHandle {
    File(file::FileBackend),
    Mysql(mysql::MySqlBackend),
}

impl WorkspaceBackend for BackendHandle {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String> {
        match self {
            BackendHandle::File(b) => b.read(path, ranges),
            BackendHandle::Mysql(b) => b.read(path, ranges),
        }
    }

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
    ) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.write(path, ranges, content, created_by, desc),
            BackendHandle::Mysql(b) => b.write(path, ranges, content, created_by, desc),
        }
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        match self {
            BackendHandle::File(b) => b.list(scope),
            BackendHandle::Mysql(b) => b.list(scope),
        }
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.remove(path),
            BackendHandle::Mysql(b) => b.remove(path),
        }
    }
}

pub fn open_backend(config: &crate::config::Config) -> WsResult<BackendHandle> {
    match &config.backend {
        crate::config::BackendConfig::File {
            workspace_dir,
            metadata_suffix,
        } => Ok(BackendHandle::File(file::FileBackend::new(
            workspace_dir.clone(),
            metadata_suffix.clone(),
        ))),
        crate::config::BackendConfig::Mysql {
            host,
            port,
            user,
            password,
            database,
        } => Ok(BackendHandle::Mysql(
            mysql::MySqlBackend::connect(host, *port, user, password, database)?,
        )),
    }
}
