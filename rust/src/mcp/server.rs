//! Synchronous stdio JSON-RPC loop and method dispatch for the MCP server.
//!
//! The loop is intentionally synchronous: the MySQL backend creates and drives
//! its own Tokio runtime via `block_on`, so running this loop inside a runtime
//! would panic ("cannot start a runtime from within a runtime").

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

use super::protocol::{Request, Response, RpcError, PARSE_ERROR, SUPPORTED_PROTOCOL_VERSION};
use super::tools;
use crate::config::Config;
use crate::error::WsError;

/// Run the MCP server over stdin/stdout until EOF.
pub fn run(config: &Config) -> Result<(), WsError> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    let mut line = String::new();
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).map_err(WsError::Io)?;
        if bytes == 0 {
            break; // EOF
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(response) = handle_line(trimmed, config) {
            let serialized = serde_json::to_string(&response)
                .map_err(|e| WsError::Other(format!("failed to serialize response: {e}")))?;
            writeln!(writer, "{serialized}").map_err(WsError::Io)?;
            writer.flush().map_err(WsError::Io)?;
        }
    }

    Ok(())
}

/// Parse a single line and produce a response, or `None` for notifications.
fn handle_line(line: &str, config: &Config) -> Option<Response> {
    let request: Request = match serde_json::from_str(line) {
        Ok(req) => req,
        Err(e) => {
            return Some(Response::failure(
                Value::Null,
                RpcError::new(PARSE_ERROR, format!("parse error: {e}")),
            ));
        }
    };

    let is_notification = request.is_notification();
    let id = request.id.clone().unwrap_or(Value::Null);
    let result = dispatch(&request.method, request.params, config);

    // Notifications never receive a response, even on error.
    if is_notification {
        return None;
    }

    Some(match result {
        Ok(value) => Response::success(id, value),
        Err(error) => Response::failure(id, error),
    })
}

/// Route a JSON-RPC method to its handler.
fn dispatch(method: &str, params: Value, config: &Config) -> Result<Value, RpcError> {
    match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools::tool_definitions() })),
        "tools/call" => tools::call(params, config),
        // Known notifications we simply accept.
        m if m.starts_with("notifications/") => Ok(Value::Null),
        other => Err(RpcError::method_not_found(other)),
    }
}

/// Build the `initialize` result, echoing the client's protocol version when
/// provided so negotiation succeeds with a range of clients.
fn initialize_result(params: &Value) -> Value {
    let protocol_version = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(SUPPORTED_PROTOCOL_VERSION);

    json!({
        "protocolVersion": protocol_version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "agent-workspace",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BackendConfig, Config};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn test_config(dir: &TempDir) -> Config {
        Config {
            config_path: PathBuf::from("config.yaml"),
            backend: BackendConfig::File {
                workspace_dir: dir.path().to_path_buf(),
                metadata_suffix: ".meta.yaml".to_string(),
            },
        }
    }

    fn parse(line: &str, config: &Config) -> Value {
        let response = handle_line(line, config).expect("expected a response");
        serde_json::to_value(response).unwrap()
    }

    #[test]
    fn initialize_returns_server_info_and_echoes_version() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
            &config,
        );
        assert_eq!(resp["id"], json!(1));
        assert_eq!(resp["result"]["protocolVersion"], json!("2025-03-26"));
        assert_eq!(resp["result"]["serverInfo"]["name"], json!("agent-workspace"));
        assert!(resp["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_produce_no_response() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let out = handle_line(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            &config,
        );
        assert!(out.is_none());
    }

    #[test]
    fn tools_list_includes_all_workspace_tools() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#, &config);
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"read"));
        assert!(names.contains(&"write"));
        assert!(names.contains(&"list"));
        assert!(names.contains(&"remove"));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse(r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#, &config);
        assert_eq!(resp["error"]["code"], json!(-32601));
    }

    #[test]
    fn tools_call_write_then_read_roundtrips() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);

        let write = parse(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"write","arguments":{"path":"a.txt","content":"line1\nline2\n","created_by":"tester","desc":"demo"}}}"#,
            &config,
        );
        assert_eq!(write["result"]["isError"], json!(false));

        let read = parse(
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"read","arguments":{"path":"a.txt"}}}"#,
            &config,
        );
        assert_eq!(read["result"]["isError"], json!(false));
        assert_eq!(read["result"]["content"][0]["text"], json!("line1\nline2\n"));

        // Range read returns only the requested line (backend filters once).
        let ranged = parse(
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"read","arguments":{"path":"a.txt","ranges":"2-2"}}}"#,
            &config,
        );
        assert_eq!(ranged["result"]["content"][0]["text"], json!("line2\n"));
    }

    #[test]
    fn tools_call_missing_argument_is_reported_as_iserror() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse(
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"read","arguments":{}}}"#,
            &config,
        );
        assert_eq!(resp["result"]["isError"], json!(true));
    }

    #[test]
    fn unknown_tool_returns_invalid_params() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse(
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
            &config,
        );
        assert_eq!(resp["error"]["code"], json!(-32602));
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let dir = TempDir::new().unwrap();
        let config = test_config(&dir);
        let resp = parse("{not json", &config);
        assert_eq!(resp["error"]["code"], json!(-32700));
    }
}
