use crate::backend::{ListReport, WorkspaceBackend};
use crate::commands::ranges::LineRange;
use crate::error::{WsError, WsResult};

#[allow(dead_code)]
fn map_db_err(e: sqlx::Error) -> WsError {
    if let sqlx::Error::Database(db_err) = &e {
        if matches!(
            db_err.code().map(|c| c.to_string()).as_deref(),
            Some("1205") | Some("1213")
        ) {
            return WsError::LockConflict(db_err.message().to_string());
        }
    }
    let msg = e.to_string();
    if msg.contains("Lock wait timeout") {
        return WsError::LockConflict(msg);
    }
    WsError::Other(msg)
}

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
