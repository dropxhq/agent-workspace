use std::path::Path;

use clap::{CommandFactory, Parser, Subcommand};

use crate::storage::{open_scoped_backend, BackendHandle};
use crate::commands;
use crate::config::Config;
use crate::error::WsError;
use crate::scoping::SessionScope;

#[derive(Parser)]
#[command(
    name = "ws",
    about = "Agent workspace file operations (restricted to configured workspace)",
)]
struct Cli {
    /// Path to config.yaml (defaults to AGENT_WORKSPACE_CONFIG or ./config.yaml in cwd)
    #[arg(long, global = true)]
    config: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new workspace (config.yaml + backend layout)
    Init {
        /// Target directory (defaults to current working directory)
        path: Option<String>,
        /// Backend type: file or mysql
        #[arg(long, default_value = "file")]
        backend: String,
    },
    /// Read a file from the workspace
    Read {
        /// Workspace-relative path
        path: String,
        /// 1-indexed line ranges, comma-separated (e.g. 1-10,20-30)
        #[arg(long)]
        ranges: Option<String>,
        /// Human-readable output with line numbers
        #[arg(long)]
        human: bool,
        /// User identifier; scopes root to workspace_dir/user_id (or .../user_id/session_id with --session-id)
        #[arg(long)]
        user_id: Option<String>,
        /// Session identifier; with --user-id scopes root to workspace_dir/user_id/session_id
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Write content to a workspace file (stdin or --content)
    Write {
        /// Workspace-relative path
        path: String,
        /// Replace lines START-END (1-indexed, inclusive) with new content
        #[arg(long)]
        ranges: Option<String>,
        /// Creator identifier stored in metadata
        #[arg(long)]
        created_by: String,
        /// Description stored in metadata
        #[arg(long)]
        desc: String,
        /// Content to write
        #[arg(long)]
        content: String,
        /// User identifier; scopes root to workspace_dir/user_id (or .../user_id/session_id with --session-id)
        #[arg(long)]
        user_id: Option<String>,
        /// Session identifier; with --user-id scopes root to workspace_dir/user_id/session_id
        #[arg(long)]
        session_id: Option<String>,
    },
    /// List workspace files (optionally scoped to a subdirectory)
    List {
        /// Subdirectory relative path (omit to list entire workspace)
        path: Option<String>,
        /// Output JSON
        #[arg(long)]
        json: bool,
        /// User identifier; scopes root to workspace_dir/user_id (or .../user_id/session_id with --session-id)
        #[arg(long)]
        user_id: Option<String>,
        /// Session identifier; with --user-id scopes root to workspace_dir/user_id/session_id
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Remove a file and its metadata sidecar
    Remove {
        /// Workspace-relative path
        path: String,
    },
    /// Run a local MCP server over stdio (exposes workspace ops as MCP tools)
    Mcp,
}

pub fn run() -> Result<(), WsError> {
    let cli = Cli::parse();

    let Some(command) = cli.command else {
        let mut cmd = Cli::command();
        cmd.print_help()?;
        println!();
        return Ok(());
    };

    match command {
        Commands::Init { path, backend } => commands::init::run(path.as_deref(), &backend),
        Commands::Mcp => {
            let config = load_config(cli.config.as_deref())?;
            crate::mcp::run(&config)?;
            Ok(())
        }
        command => {
            let config = load_config(cli.config.as_deref())?;
            dispatch(command, &config)
        }
    }
}

fn load_config(config_path: Option<&str>) -> Result<Config, WsError> {
    match config_path {
        Some(path) => Config::load_from_path(Path::new(path)),
        None => Config::load(),
    }
}

fn dispatch(command: Commands, config: &Config) -> Result<(), WsError> {
    match command {
        Commands::Init { .. } => unreachable!(),
        Commands::Read {
            path,
            ranges,
            human,
            user_id,
            session_id,
        } => {
            let backend = scoped_backend(config, user_id.as_deref(), session_id.as_deref())?;
            commands::read::run(&path, ranges.as_deref(), human, &backend)?
        }
        Commands::Write {
            path,
            ranges,
            created_by,
            desc,
            content,
            user_id,
            session_id,
        } => {
            let backend = scoped_backend(config, user_id.as_deref(), session_id.as_deref())?;
            commands::write::run(
                &path,
                ranges.as_deref(),
                &created_by,
                &desc,
                &content,
                &backend,
            )?
        }
        Commands::List {
            path,
            json,
            user_id,
            session_id,
        } => {
            let backend = scoped_backend(config, user_id.as_deref(), session_id.as_deref())?;
            commands::list::run(path.as_deref(), json, &backend)?
        }
        Commands::Remove { path } => {
            let backend = open_scoped_backend(config, SessionScope::default())?;
            commands::remove::run(&path, &backend)?
        }
        Commands::Mcp => unreachable!(),
    }

    Ok(())
}

fn scoped_backend(
    config: &Config,
    user_id: Option<&str>,
    session_id: Option<&str>,
) -> Result<BackendHandle, WsError> {
    let scope = SessionScope::from_options(user_id, session_id)?;
    open_scoped_backend(config, scope)
}
