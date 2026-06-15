"""agent-workspace: restricted file-operation workspace for AI agents.

Native bindings over the Rust core. All paths are workspace-relative and
cannot escape the configured workspace root.

Example
-------
>>> import agent_workspace as ws
>>> ws.init("./my-workspace")              # writes config.yaml + data/
>>> w = ws.Workspace("./my-workspace/config.yaml")
>>> w.write("docs/a.txt", "hello\\n", created_by="me", desc="demo")
>>> w.read("docs/a.txt")
'hello\\n'
>>> report = w.list()
>>> report.file_count
1
>>> w.remove("docs/a.txt")
"""

from ._agent_workspace import (
    FileMeta,
    InvalidPathError,
    InvalidRangesError,
    ListReport,
    LockConflictError,
    NotFoundError,
    PathEscapeError,
    Workspace,
    WorkspaceError,
    __version__,
    init,
)

__all__ = [
    "Workspace",
    "ListReport",
    "FileMeta",
    "init",
    "WorkspaceError",
    "InvalidPathError",
    "PathEscapeError",
    "NotFoundError",
    "LockConflictError",
    "InvalidRangesError",
    "__version__",
]
