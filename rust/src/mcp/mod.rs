//! Local MCP (Model Context Protocol) server.
//!
//! Exposes the workspace read/write/list/remove operations as MCP tools over a
//! stdio JSON-RPC 2.0 transport, so MCP-capable clients can drive the workspace
//! by launching `ws mcp` as a subprocess.
//!
//! - [`protocol`]: JSON-RPC message types and error codes.
//! - [`tools`]: tool descriptors and execution (maps onto `WorkspaceBackend`).
//! - [`server`]: the stdio loop and method dispatch.

pub mod protocol;
pub mod server;
pub mod tools;

pub use server::run;
