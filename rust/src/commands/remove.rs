use crate::storage::{BackendHandle, WorkspaceBackend};
use crate::error::WsResult;

pub fn run(path: &str, backend: &BackendHandle) -> WsResult<()> {
    backend.remove(path)
}
