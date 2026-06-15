use crate::storage::mysql::MySqlBackend;
use crate::storage::{ListReport, WorkspaceBackend};
use crate::ranges::LineRange;
use crate::config::IoOptions;
use crate::error::WsResult;
use crate::scoping::SessionScope;

pub struct ScopedMySqlBackend {
    inner: MySqlBackend,
    scope: SessionScope,
}

impl ScopedMySqlBackend {
    pub fn new(inner: MySqlBackend, scope: SessionScope) -> Self {
        Self { inner, scope }
    }

    fn scoped_list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        let storage_scope = scope.map(|s| self.scope.storage_path(s));
        let storage_scope = storage_scope.as_deref();
        let mut report = self.inner.list(storage_scope)?;

        report.scope = scope.map(|s| self.scope.display_path(&self.scope.storage_path(s)));
        for file in &mut report.files {
            file.relative_path = self.scope.display_path(&file.relative_path);
        }

        Ok(report)
    }
}

impl WorkspaceBackend for ScopedMySqlBackend {
    fn read(
        &self,
        path: &str,
        ranges: Option<&[crate::ranges::LineRange]>,
        opts: IoOptions,
    ) -> WsResult<String> {
        self.inner
            .read(&self.scope.storage_path(path), ranges, opts)
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
        self.inner.write(
            &self.scope.storage_path(path),
            ranges,
            content,
            created_by,
            desc,
            opts,
        )
    }

    fn list(&self, scope: Option<&str>) -> WsResult<ListReport> {
        self.scoped_list(scope)
    }

    fn remove(&self, path: &str) -> WsResult<()> {
        self.inner.remove(&self.scope.storage_path(path))
    }
}
