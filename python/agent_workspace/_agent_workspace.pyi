from typing import Optional

__version__: str

class FileMeta:
    """Metadata for a single workspace file."""

    relative_path: str
    created_by: str
    desc: str
    created_at: str  # RFC 3339 timestamp
    updated_at: str  # RFC 3339 timestamp
    size_bytes: int
    sha256: Optional[str]

class ListReport:
    """Result of :meth:`Workspace.list`."""

    scope: Optional[str]
    file_count: int
    total_size_bytes: int
    files: list[FileMeta]

class Workspace:
    """A handle to a workspace backend, scoped to an optional user/session.

    All paths are workspace-relative and cannot escape the workspace root.
    """

    config_path: str

    def __init__(
        self,
        config_path: Optional[str] = ...,
        *,
        user_id: Optional[str] = ...,
        session_id: Optional[str] = ...,
    ) -> None:
        """Open a workspace.

        :param config_path: path to ``config.yaml``. If ``None``, resolves via
            ``AGENT_WORKSPACE_CONFIG`` or ``./config.yaml``.
        :param user_id: optional scoping segment.
        :param session_id: optional scoping segment (combined with ``user_id``).
        """

    def read(self, path: str, ranges: Optional[str] = ...) -> str:
        """Read a file. ``ranges`` is a 1-indexed, comma-separated spec
        (e.g. ``"1-10,20-30"``); when given, only those lines are returned."""

    def write(
        self,
        path: str,
        content: str,
        *,
        created_by: str,
        desc: str,
        ranges: Optional[str] = ...,
    ) -> None:
        """Write ``content`` to ``path``. With ``ranges`` (a single
        ``"START-END"``), replaces those lines instead of overwriting."""

    def list(self, scope: Optional[str] = ...) -> ListReport:
        """List files, optionally restricted to a subdirectory ``scope``."""

    def remove(self, path: str) -> None:
        """Remove a file and its metadata."""

def init(target: Optional[str] = ..., *, backend: str = ...) -> None:
    """Initialize a new workspace (writes ``config.yaml``; for the file backend
    also creates ``data/``). ``backend`` is ``"file"`` (default) or ``"mysql"``."""

class WorkspaceError(Exception):
    """Base class for all workspace errors."""

class InvalidPathError(WorkspaceError): ...
class PathEscapeError(WorkspaceError): ...
class NotFoundError(WorkspaceError): ...
class LockConflictError(WorkspaceError): ...
class InvalidRangesError(WorkspaceError): ...
