//! Python bindings (PyO3).
//!
//! Compiled only with `--features python` and exposed by maturin as the native
//! extension module `agent_workspace._agent_workspace`. The pure-Python package
//! `agent_workspace` (see `python/`) re-exports these symbols.
//!
//! The bindings wrap [`WorkspaceBackend`] exactly like the MCP tools do: a
//! [`Workspace`] opens a scoped backend from a config file and forwards
//! read/write/list/remove. `WsError` variants map onto dedicated Python
//! exception types so callers can `except` on them.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use crate::commands;
use crate::config::Config;
use crate::error::WsError;
use crate::ranges::parse_ranges;
use crate::scoping::SessionScope;
use crate::storage::{open_scoped_backend, BackendHandle, ListReport as CoreListReport, WorkspaceBackend};

use std::path::Path;

// --- Exception hierarchy -------------------------------------------------

create_exception!(_agent_workspace, WorkspaceError, PyException, "Base class for all workspace errors.");
create_exception!(_agent_workspace, InvalidPathError, WorkspaceError, "Path is invalid (exit code 2).");
create_exception!(_agent_workspace, PathEscapeError, WorkspaceError, "Path escapes the workspace root (exit code 2).");
create_exception!(_agent_workspace, NotFoundError, WorkspaceError, "File or metadata not found (exit code 3).");
create_exception!(_agent_workspace, LockConflictError, WorkspaceError, "Concurrent lock conflict (exit code 4).");
create_exception!(_agent_workspace, InvalidRangesError, WorkspaceError, "Invalid line ranges.");

/// Map a [`WsError`] onto the matching Python exception, preserving the message
/// and the CLI exit-code semantics.
fn to_pyerr(err: WsError) -> PyErr {
    let msg = err.to_string();
    match err {
        WsError::InvalidPath(_) => InvalidPathError::new_err(msg),
        WsError::PathEscape(_) => PathEscapeError::new_err(msg),
        WsError::NotFound(_) => NotFoundError::new_err(msg),
        WsError::LockConflict(_) => LockConflictError::new_err(msg),
        WsError::InvalidRanges(_) => InvalidRangesError::new_err(msg),
        WsError::Io(_) | WsError::Other(_) => WorkspaceError::new_err(msg),
    }
}

// --- Result types --------------------------------------------------------

/// Metadata for a single workspace file (mirrors the Rust `FileMetadata`).
#[pyclass(module = "agent_workspace", frozen)]
#[derive(Clone)]
pub struct FileMeta {
    #[pyo3(get)]
    pub relative_path: String,
    #[pyo3(get)]
    pub created_by: String,
    #[pyo3(get)]
    pub desc: String,
    /// RFC 3339 timestamp string.
    #[pyo3(get)]
    pub created_at: String,
    /// RFC 3339 timestamp string.
    #[pyo3(get)]
    pub updated_at: String,
    #[pyo3(get)]
    pub size_bytes: u64,
    #[pyo3(get)]
    pub sha256: Option<String>,
}

#[pymethods]
impl FileMeta {
    fn __repr__(&self) -> String {
        format!(
            "FileMeta(relative_path={:?}, created_by={:?}, size_bytes={}, updated_at={:?})",
            self.relative_path, self.created_by, self.size_bytes, self.updated_at
        )
    }
}

/// Result of [`Workspace.list`].
#[pyclass(module = "agent_workspace", frozen)]
pub struct ListReport {
    /// The scope prefix this listing was restricted to, if any.
    #[pyo3(get)]
    pub scope: Option<String>,
    #[pyo3(get)]
    pub file_count: usize,
    #[pyo3(get)]
    pub total_size_bytes: u64,
    #[pyo3(get)]
    pub files: Vec<FileMeta>,
}

impl From<CoreListReport> for ListReport {
    fn from(report: CoreListReport) -> Self {
        let files = report
            .files
            .into_iter()
            .map(|m| FileMeta {
                relative_path: m.relative_path,
                created_by: m.created_by,
                desc: m.desc,
                created_at: m.created_at.to_rfc3339(),
                updated_at: m.updated_at.to_rfc3339(),
                size_bytes: m.size_bytes,
                sha256: m.sha256,
            })
            .collect();
        ListReport {
            scope: report.scope,
            file_count: report.file_count,
            total_size_bytes: report.total_size_bytes,
            files,
        }
    }
}

#[pymethods]
impl ListReport {
    fn __repr__(&self) -> String {
        format!(
            "ListReport(scope={:?}, file_count={}, total_size_bytes={})",
            self.scope, self.file_count, self.total_size_bytes
        )
    }
}

// --- Workspace -----------------------------------------------------------

/// A handle to a workspace backend, scoped to an optional user/session.
///
/// The backend (file or mysql) is opened once from the config file and reused
/// across calls. All paths are workspace-relative and cannot escape the root.
#[pyclass(module = "agent_workspace")]
pub struct Workspace {
    backend: BackendHandle,
    #[pyo3(get)]
    config_path: String,
}

#[pymethods]
impl Workspace {
    /// Open a workspace.
    ///
    /// - `config_path`: path to `config.yaml`. If `None`, resolves via
    ///   `AGENT_WORKSPACE_CONFIG` or `./config.yaml` (same as the CLI).
    /// - `user_id` / `session_id`: optional scoping. Both → `user/session`
    ///   subpath; only `user_id` → `user` subpath; otherwise no scoping.
    #[new]
    #[pyo3(signature = (config_path=None, *, user_id=None, session_id=None))]
    fn new(
        config_path: Option<&str>,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> PyResult<Self> {
        let config = match config_path {
            Some(p) => Config::load_from_path(Path::new(p)),
            None => Config::load(),
        }
        .map_err(to_pyerr)?;

        let resolved_path = config.config_path.display().to_string();
        let scope = SessionScope::from_options(user_id, session_id).map_err(to_pyerr)?;
        let backend = open_scoped_backend(&config, scope).map_err(to_pyerr)?;

        Ok(Workspace {
            backend,
            config_path: resolved_path,
        })
    }

    /// Read a file. `ranges` is an optional 1-indexed, comma-separated spec
    /// (e.g. `"1-10,20-30"`); when given, only those lines are returned.
    #[pyo3(signature = (path, ranges=None))]
    fn read(&self, py: Python<'_>, path: &str, ranges: Option<&str>) -> PyResult<String> {
        let parsed = ranges.map(parse_ranges).transpose().map_err(to_pyerr)?;
        py.allow_threads(|| self.backend.read(path, parsed.as_deref()))
            .map_err(to_pyerr)
    }

    /// Write `content` to `path`.
    ///
    /// With `ranges` (a single `"START-END"`), replaces those lines instead of
    /// overwriting the whole file. `created_by`/`desc` are stored in metadata.
    #[pyo3(signature = (path, content, *, created_by, desc, ranges=None))]
    fn write(
        &self,
        py: Python<'_>,
        path: &str,
        content: &str,
        created_by: &str,
        desc: &str,
        ranges: Option<&str>,
    ) -> PyResult<()> {
        let parsed_range = match ranges {
            Some(raw) => {
                let mut parsed = parse_ranges(raw).map_err(to_pyerr)?;
                if parsed.len() > 1 {
                    return Err(InvalidRangesError::new_err(
                        "write supports only a single range (START-END)",
                    ));
                }
                parsed.pop()
            }
            None => None,
        };

        py.allow_threads(|| {
            self.backend
                .write(path, parsed_range.as_ref(), content, created_by, desc)
        })
        .map_err(to_pyerr)
    }

    /// List files, optionally restricted to a subdirectory `scope`.
    #[pyo3(signature = (scope=None))]
    fn list(&self, py: Python<'_>, scope: Option<&str>) -> PyResult<ListReport> {
        let report = py
            .allow_threads(|| self.backend.list(scope))
            .map_err(to_pyerr)?;
        Ok(ListReport::from(report))
    }

    /// Remove a file and its metadata.
    fn remove(&self, py: Python<'_>, path: &str) -> PyResult<()> {
        py.allow_threads(|| self.backend.remove(path))
            .map_err(to_pyerr)
    }

    fn __repr__(&self) -> String {
        format!("Workspace(config_path={:?})", self.config_path)
    }
}

// --- Module-level functions ----------------------------------------------

/// Initialize a new workspace (writes `config.yaml`; for the file backend also
/// creates `data/`). `backend` is `"file"` (default) or `"mysql"`.
#[pyfunction]
#[pyo3(signature = (target=None, *, backend="file"))]
fn init(target: Option<&str>, backend: &str) -> PyResult<()> {
    commands::init::run(target, backend).map_err(to_pyerr)
}

/// The native extension module (`agent_workspace._agent_workspace`).
#[pymodule]
fn _agent_workspace(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();

    m.add_class::<Workspace>()?;
    m.add_class::<ListReport>()?;
    m.add_class::<FileMeta>()?;
    m.add_function(wrap_pyfunction!(init, m)?)?;

    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    m.add("WorkspaceError", py.get_type_bound::<WorkspaceError>())?;
    m.add("InvalidPathError", py.get_type_bound::<InvalidPathError>())?;
    m.add("PathEscapeError", py.get_type_bound::<PathEscapeError>())?;
    m.add("NotFoundError", py.get_type_bound::<NotFoundError>())?;
    m.add("LockConflictError", py.get_type_bound::<LockConflictError>())?;
    m.add("InvalidRangesError", py.get_type_bound::<InvalidRangesError>())?;

    Ok(())
}
