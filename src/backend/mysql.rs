use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::LineRange;
use crate::error::{WsError, WsResult};

pub struct MySqlBackend;

impl MySqlBackend {
    pub fn connect(
        _host: &str,
        _port: u16,
        _user: &str,
        _password: &str,
        _database: &str,
    ) -> WsResult<Self> {
        Ok(Self)
    }
}

impl WorkspaceBackend for MySqlBackend {
    fn read(
        &self,
        _path: &str,
        _ranges: Option<&[crate::commands::ranges::LineRange]>,
    ) -> WsResult<String> {
        Err(WsError::Other("MySqlBackend::read not implemented".into()))
    }

    fn write(
        &self,
        _path: &str,
        _ranges: Option<&LineRange>,
        _content: &str,
        _created_by: &str,
        _desc: &str,
    ) -> WsResult<()> {
        Err(WsError::Other("MySqlBackend::write not implemented".into()))
    }

    fn list(&self, _scope: Option<&str>) -> WsResult<ListReport> {
        Err(WsError::Other("MySqlBackend::list not implemented".into()))
    }

    fn remove(&self, _path: &str) -> WsResult<()> {
        Err(WsError::Other("MySqlBackend::remove not implemented".into()))
    }
}
