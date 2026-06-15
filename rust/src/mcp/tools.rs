//! MCP tool definitions and execution.
//!
//! Tools map directly onto [`WorkspaceBackend`] methods. We deliberately do
//! **not** reuse `crate::commands::*::run`, because those write results to
//! stdout — which is the JSON-RPC transport channel here and must stay clean.
//!
//! Per the MCP spec, tool *execution* failures are reported as a successful
//! `tools/call` result with `isError: true` (so the model can see and recover
//! from them), rather than as JSON-RPC protocol errors.

use serde_json::{json, Value};

use super::protocol::{RpcError, INVALID_PARAMS};
use crate::config::{Config, IoOptions};
use crate::error::WsError;
use crate::ranges::parse_ranges;
use crate::scoping::SessionScope;
use crate::storage::{open_scoped_backend, WorkspaceBackend};

/// JSON Schema fragment shared by all tools: optional user/session scoping.
fn scope_properties() -> Value {
    json!({
        "user_id": {
            "type": "string",
            "description": "Optional user id; scopes the root to workspace_dir/user_id (or .../user_id/session_id with session_id)."
        },
        "session_id": {
            "type": "string",
            "description": "Optional session id; combined with user_id scopes the root to workspace_dir/user_id/session_id."
        }
    })
}

/// Tool descriptors returned by `tools/list`.
pub fn tool_definitions() -> Value {
    let scope = scope_properties();
    json!([
        {
            "name": "read",
            "description": "Read a file from the workspace. Optionally filter by 1-indexed line ranges.",
            "inputSchema": {
                "type": "object",
                "properties": merge(json!({
                    "path": { "type": "string", "description": "Workspace-relative path." },
                    "ranges": { "type": "string", "description": "Optional 1-indexed line ranges, comma-separated (e.g. 1-10,20-30)." },
                    "skip_hooks": { "type": "boolean", "description": "If true, bypass configured read/write hooks for this call." }
                }), &scope),
                "required": ["path"]
            }
        },
        {
            "name": "write",
            "description": "Write content to a workspace file. With `ranges` (a single START-END), replaces those lines instead of overwriting the whole file.",
            "inputSchema": {
                "type": "object",
                "properties": merge(json!({
                    "path": { "type": "string", "description": "Workspace-relative path." },
                    "content": { "type": "string", "description": "Content to write." },
                    "created_by": { "type": "string", "description": "Creator identifier stored in metadata." },
                    "desc": { "type": "string", "description": "Description stored in metadata." },
                    "ranges": { "type": "string", "description": "Optional single range START-END (1-indexed, inclusive) to replace." },
                    "skip_hooks": { "type": "boolean", "description": "If true, bypass configured read/write hooks for this call." }
                }), &scope),
                "required": ["path", "content", "created_by", "desc"]
            }
        },
        {
            "name": "list",
            "description": "List workspace files (optionally scoped to a subdirectory). Returns a JSON report.",
            "inputSchema": {
                "type": "object",
                "properties": merge(json!({
                    "path": { "type": "string", "description": "Optional subdirectory relative path (omit to list the entire workspace)." }
                }), &scope),
                "required": []
            }
        },
        {
            "name": "remove",
            "description": "Remove a file and its metadata sidecar.",
            "inputSchema": {
                "type": "object",
                "properties": merge(json!({
                    "path": { "type": "string", "description": "Workspace-relative path." }
                }), &scope),
                "required": ["path"]
            }
        }
    ])
}

/// Merge `extra` properties into `base` (both must be JSON objects).
fn merge(mut base: Value, extra: &Value) -> Value {
    if let (Some(base_map), Some(extra_map)) = (base.as_object_mut(), extra.as_object()) {
        for (k, v) in extra_map {
            base_map.insert(k.clone(), v.clone());
        }
    }
    base
}

/// Handle a `tools/call` request.
///
/// Returns a JSON-RPC error only for malformed calls (e.g. missing tool name or
/// unknown tool). Backend/validation failures are encoded as `isError` results.
pub fn call(params: Value, config: &Config) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| RpcError::new(INVALID_PARAMS, "missing tool name"))?;
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

    let result = match name {
        "read" => read_tool(&args, config),
        "write" => write_tool(&args, config),
        "list" => list_tool(&args, config),
        "remove" => remove_tool(&args, config),
        other => return Err(RpcError::new(INVALID_PARAMS, format!("unknown tool: {other}"))),
    };

    Ok(match result {
        Ok(text) => text_result(text, false),
        Err(e) => text_result(format!("error: {e}"), true),
    })
}

/// Build a `tools/call` result with a single text content block.
fn text_result(text: String, is_error: bool) -> Value {
    json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": is_error
    })
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, WsError> {
    arg_str(args, key)
        .ok_or_else(|| WsError::Other(format!("missing required argument: {key}")))
}

fn scope_of(args: &Value) -> Result<SessionScope, WsError> {
    SessionScope::from_options(arg_str(args, "user_id"), arg_str(args, "session_id"))
}

fn io_options(args: &Value) -> IoOptions {
    IoOptions {
        skip_hooks: args
            .get("skip_hooks")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn read_tool(args: &Value, config: &Config) -> Result<String, WsError> {
    let path = required_str(args, "path")?;
    let parsed = arg_str(args, "ranges").map(parse_ranges).transpose()?;
    let backend = open_scoped_backend(config, scope_of(args)?)?;
    // The backend already applies range filtering when ranges are provided.
    backend.read(path, parsed.as_deref(), io_options(args))
}

fn write_tool(args: &Value, config: &Config) -> Result<String, WsError> {
    let path = required_str(args, "path")?;
    let content = required_str(args, "content")?;
    let created_by = required_str(args, "created_by")?;
    let desc = required_str(args, "desc")?;

    let parsed_range = match arg_str(args, "ranges") {
        Some(raw) => {
            let mut ranges = parse_ranges(raw)?;
            if ranges.len() > 1 {
                return Err(WsError::InvalidRanges(
                    "write supports only a single range (START-END)".to_string(),
                ));
            }
            ranges.pop()
        }
        None => None,
    };

    let backend = open_scoped_backend(config, scope_of(args)?)?;
    backend.write(
        path,
        parsed_range.as_ref(),
        content,
        created_by,
        desc,
        io_options(args),
    )?;
    Ok(format!("wrote {} bytes to {path}", content.len()))
}

fn list_tool(args: &Value, config: &Config) -> Result<String, WsError> {
    let backend = open_scoped_backend(config, scope_of(args)?)?;
    let report = backend.list(arg_str(args, "path"))?;
    serde_json::to_string_pretty(&report)
        .map_err(|e| WsError::Other(format!("json serialize failed: {e}")))
}

fn remove_tool(args: &Value, config: &Config) -> Result<String, WsError> {
    let path = required_str(args, "path")?;
    let backend = open_scoped_backend(config, scope_of(args)?)?;
    backend.remove(path)?;
    Ok(format!("removed {path}"))
}
