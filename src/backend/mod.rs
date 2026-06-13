use crate::commands::ranges::LineRange;
use crate::error::WsResult;
use std::fs;
use crate::meta::FileMetadata;

pub mod content;
pub mod file;
pub mod mysql;
pub mod path;
pub mod scoped;

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
    ScopedMysql(scoped::ScopedMySqlBackend),
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
            BackendHandle::ScopedMysql(b) => b.read(path, ranges),
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
            BackendHandle::ScopedMysql(b) => b.write(path, ranges, content, created_by, desc),
        }
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        match self {
            BackendHandle::File(b) => b.list(scope),
            BackendHandle::Mysql(b) => b.list(scope),
            BackendHandle::ScopedMysql(b) => b.list(scope),
        }
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.remove(path),
            BackendHandle::Mysql(b) => b.remove(path),
            BackendHandle::ScopedMysql(b) => b.remove(path),
        }
    }
}

pub fn open_scoped_backend(
    config: &crate::config::Config,
    scope: crate::workspace::SessionScope,
) -> WsResult<BackendHandle> {
    let backend = open_backend(config)?;
    apply_session_scope(backend, scope)
}

fn apply_session_scope(
    backend: BackendHandle,
    scope: crate::workspace::SessionScope,
) -> WsResult<BackendHandle> {
    if scope.prefix().is_none() {
        return Ok(backend);
    }

    match backend {
        BackendHandle::File(file_backend) => {
            let root = scope.effective_root(&file_backend.workspace_dir);
            fs::create_dir_all(&root).map_err(crate::error::WsError::Io)?;
            Ok(BackendHandle::File(file::FileBackend::new(
                root,
                file_backend.metadata_suffix,
            )))
        }
        BackendHandle::Mysql(mysql_backend) => Ok(BackendHandle::ScopedMysql(
            scoped::ScopedMySqlBackend::new(mysql_backend, scope),
        )),
        BackendHandle::ScopedMysql(_) => unreachable!("scoped mysql backend is not nested"),
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
        } => Ok(BackendHandle::Mysql(mysql::MySqlBackend::connect(
            host, *port, user, password, database,
        )?)),
    }
}
