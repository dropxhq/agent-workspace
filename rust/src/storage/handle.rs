use std::fs;

use crate::config::{Config, IoOptions};
use crate::error::WsResult;
use crate::ranges::LineRange;
use crate::scoping::SessionScope;
use crate::storage::{file, hooked, mysql, scoped, ListReport, WorkspaceBackend};

pub enum BackendHandle {
    File(file::FileBackend),
    Mysql(mysql::MySqlBackend),
    ScopedMysql(scoped::ScopedMySqlBackend),
    Hooked(hooked::HookedBackend),
}

impl WorkspaceBackend for BackendHandle {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[LineRange]>,
        opts: IoOptions,
    ) -> WsResult<String> {
        match self {
            BackendHandle::File(b) => b.read(path, ranges, opts),
            BackendHandle::Mysql(b) => b.read(path, ranges, opts),
            BackendHandle::ScopedMysql(b) => b.read(path, ranges, opts),
            BackendHandle::Hooked(b) => b.read(path, ranges, opts),
        }
    }

    fn write(
        &self,
        path: &str,
        ranges: Option<&LineRange>,
        content: &str,
        created_by: &str,
        desc: &str,
        opts: IoOptions,
    ) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.write(path, ranges, content, created_by, desc, opts),
            BackendHandle::Mysql(b) => b.write(path, ranges, content, created_by, desc, opts),
            BackendHandle::ScopedMysql(b) => b.write(path, ranges, content, created_by, desc, opts),
            BackendHandle::Hooked(b) => b.write(path, ranges, content, created_by, desc, opts),
        }
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        match self {
            BackendHandle::File(b) => b.list(scope),
            BackendHandle::Mysql(b) => b.list(scope),
            BackendHandle::ScopedMysql(b) => b.list(scope),
            BackendHandle::Hooked(b) => b.list(scope),
        }
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        match self {
            BackendHandle::File(b) => b.remove(path),
            BackendHandle::Mysql(b) => b.remove(path),
            BackendHandle::ScopedMysql(b) => b.remove(path),
            BackendHandle::Hooked(b) => b.remove(path),
        }
    }
}

pub fn open_scoped_backend(config: &Config, scope: SessionScope) -> WsResult<BackendHandle> {
    let backend = open_backend(config)?;
    let backend = apply_session_scope(backend, scope)?;
    maybe_wrap_hooks(backend, config)
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
        BackendHandle::Hooked(_hooked) => Err(crate::error::WsError::Other(
            "session scope cannot be applied to an already hooked backend".to_string(),
        )),
        BackendHandle::ScopedMysql(_) => {
            unreachable!("scoped mysql backend is not nested")
        }
    }
}

pub fn open_backend(config: &Config) -> WsResult<BackendHandle> {
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

fn maybe_wrap_hooks(backend: BackendHandle, config: &Config) -> WsResult<BackendHandle> {
    let Some(hooks) = &config.hooks else {
        return Ok(backend);
    };
    if hooks.is_empty() {
        return Ok(backend);
    }

    Ok(BackendHandle::Hooked(hooked::HookedBackend::new(
        backend,
        hooks.clone(),
        config.config_dir(),
    )))
}
