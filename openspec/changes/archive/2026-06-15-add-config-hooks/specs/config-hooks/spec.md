## ADDED Requirements

### Requirement: Optional hooks block in config.yaml

The system SHALL support an optional top-level `hooks` block in `config.yaml`, independent of `backend.type`. Each config file SHALL define at most one global read hook and one global write hook. When the `hooks` block is absent, read and write behavior MUST be identical to the pre-change implementation.

#### Scenario: Config without hooks

- **WHEN** `config.yaml` contains only a `backend` block and no `hooks` block
- **THEN** file read and write operations MUST NOT invoke any external command
- **THEN** no `HookedBackend` wrapper MUST be applied

#### Scenario: Config with both read and write hooks

- **WHEN** `config.yaml` contains `hooks.read.command` and `hooks.write.command` as non-empty argv arrays
- **THEN** the system MUST load both hook definitions at backend open time
- **THEN** read and write operations MUST apply the configured hooks unless `skip_hooks` is true

### Requirement: External command hook protocol

Each hook entry SHALL specify `command` as a non-empty JSON/YAML array of strings interpreted as an argv vector (no shell). The system MUST execute the command with:

- stdin: the full input string (UTF-8); empty content MUST be passed as an empty stdin stream
- stdout: the full output string (UTF-8) used as the hook result; the system MUST NOT trim stdout
- working directory: the directory containing `config.yaml`
- environment variables: `WS_HOOK` set to `read` or `write`, and `WS_PATH` set to the workspace-relative file path

The system MAY support optional `timeout_ms` per hook; default timeout MUST be 30000 ms when omitted.

#### Scenario: Successful hook transformation

- **WHEN** a hook command exits with code 0 within the timeout
- **THEN** the system MUST use the command's stdout as the transformed string for the next step in the pipeline

#### Scenario: Hook command failure

- **WHEN** a hook command exits with a non-zero code or exceeds the timeout
- **THEN** the read or write operation MUST fail
- **THEN** no partial write MUST be committed to storage

#### Scenario: Hook output is not valid UTF-8

- **WHEN** a hook command exits 0 but stdout is not valid UTF-8
- **THEN** the operation MUST fail with an error describing invalid hook output

### Requirement: Read hook application order

When hooks are configured and `skip_hooks` is false, the read path MUST apply the read hook after loading physical content from storage and before line-range filtering.

#### Scenario: Read without ranges

- **WHEN** a client reads a file with hooks configured and `skip_hooks` false
- **THEN** the system MUST load physical content from the backend
- **THEN** the system MUST pass physical content through the read hook to produce logical content
- **THEN** the system MUST return logical content to the caller

#### Scenario: Read with ranges

- **WHEN** a client reads a file with line ranges, hooks configured, and `skip_hooks` false
- **THEN** the system MUST apply the read hook to the full physical content first
- **THEN** the system MUST filter line ranges on the resulting logical content

### Requirement: Write hook application order

When hooks are configured and `skip_hooks` is false, the write path MUST perform line-range merging in logical space and apply the write hook immediately before persisting physical content.

#### Scenario: Full overwrite write

- **WHEN** a client writes a file without ranges, with hooks configured, and `skip_hooks` false
- **THEN** the system MUST treat caller content as logical content
- **THEN** the system MUST pass logical content through the write hook to produce physical content
- **THEN** the system MUST persist physical content and compute metadata (size, sha256) from physical bytes

#### Scenario: Range replace write

- **WHEN** a client writes with a single line range, hooks configured, and `skip_hooks` false
- **THEN** the system MUST load existing physical content and apply the read hook to obtain logical existing content
- **THEN** the system MUST merge the caller's logical new content into logical existing content using range semantics
- **THEN** the system MUST pass merged logical content through the write hook before persisting

#### Scenario: Write with skip_hooks bypasses all hooks

- **WHEN** a client writes with `skip_hooks` true
- **THEN** the system MUST NOT invoke read or write hooks for that operation
- **THEN** range merging MUST occur on physical content directly
- **THEN** physical caller content MUST be persisted without write hook transformation

### Requirement: skip_hooks per-request bypass

The system SHALL accept an optional `skip_hooks` flag on read and write operations across CLI, MCP, and Python APIs. Default MUST be `skip_hooks: false`. When `skip_hooks` is true, the system MUST NOT invoke any hook for that operation.

#### Scenario: CLI no-hooks flag

- **WHEN** a user runs `ws read PATH --no-hooks` or `ws write PATH ... --no-hooks`
- **THEN** the operation MUST run with `skip_hooks` true

#### Scenario: MCP skip_hooks argument

- **WHEN** an MCP `read` or `write` tool call includes `"skip_hooks": true`
- **THEN** the operation MUST bypass hooks for that call only

#### Scenario: Python skip_hooks keyword

- **WHEN** `Workspace.read(..., skip_hooks=True)` or `Workspace.write(..., skip_hooks=True)` is invoked
- **THEN** the operation MUST bypass hooks for that call only

#### Scenario: Read with skip_hooks returns physical content

- **WHEN** a client reads with `skip_hooks` true and hooks are configured
- **THEN** the system MUST return physical stored content (subject only to line-range filtering on physical text)

### Requirement: Hooks apply consistently across backends and entry points

Hook behavior MUST be identical for `file` and `mysql` backends. Hook execution MUST occur in the `WorkspaceBackend` layer so that CLI, MCP, and Python bindings share the same semantics without duplicating hook logic in command handlers.

#### Scenario: MCP does not duplicate hook logic in tools

- **WHEN** an MCP client calls the `read` tool
- **THEN** hook application MUST occur inside the backend implementation used by MCP, not in MCP-specific stdout command runners

### Requirement: Partial hook configuration warning

When only one of `hooks.read` or `hooks.write` is configured, the system MUST emit a warning at config load time indicating that round-trip consistency is not guaranteed.

#### Scenario: Read hook only

- **WHEN** `hooks.read` is configured but `hooks.write` is absent
- **THEN** the system MUST start successfully
- **THEN** the system MUST emit a warning about missing write hook
