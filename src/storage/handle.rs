use std::fs;

use crate::error::WsResult;
use crate::ranges::LineRange;
use crate::scoping::SessionScope;
use crate::storage::{file, mysql, scoped, ListReport, WorkspaceBackend};

pub enum BackendHandle {
    File(file::FileBackend),
    Mysql(mysql::MySqlBackend),
    ScopedMysql(scoped::ScopedMySqlBackend),
}

impl WorkspaceBackend for BackendHandle {
    fn read(&self, path: &str, ranges: Option<&[LineRange]>) -> WsResult<String> {
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
    scope: SessionScope,
) -> WsResult<BackendHandle> {
    let backend = open_backend(config)?;
    apply_session_scope(backend, scope)
}

fn apply_session_scope(backend: BackendHandle, scope: SessionScope) -> WsResult<BackendHandle> {
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
