use std::process;

use clap::{Parser, Subcommand};

use agent_workspace::commands;
use agent_workspace::config::Config;
use agent_workspace::error::WsError;

#[derive(Parser)]
#[command(name = "ws", about = "Agent workspace file operations (restricted to configured workspace)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new workspace (config.yaml + data directory)
    Init {
        /// Target directory (defaults to current working directory)
        path: Option<String>,
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
        created_by: Option<String>,
        /// Description stored in metadata
        #[arg(long)]
        desc: Option<String>,
        /// Content to write (otherwise read from stdin)
        #[arg(long)]
        content: Option<String>,
    },
    /// List workspace files (optionally scoped to a subdirectory)
    List {
        /// Subdirectory relative path (omit to list entire workspace)
        path: Option<String>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove a file and its metadata sidecar
    Remove {
        /// Workspace-relative path
        path: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        process::exit(e.exit_code().into());
    }
}

fn run() -> Result<(), WsError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { path } => commands::init::run(path.as_deref()),
        command => {
            let config = Config::load()?;
            dispatch(command, &config)
        }
    }
}

fn dispatch(command: Commands, config: &Config) -> Result<(), WsError> {
    match command {
        Commands::Init { .. } => unreachable!(),
        Commands::Read {
            path,
            ranges,
            human,
        } => commands::read::run(
            &path,
            ranges.as_deref(),
            human,
            &config,
        )?,
        Commands::Write {
            path,
            ranges,
            created_by,
            desc,
            content,
        } => commands::write::run(
            &path,
            ranges.as_deref(),
            created_by.as_deref().unwrap_or(""),
            desc.as_deref().unwrap_or(""),
            content.as_deref(),
            &config,
        )?,
        Commands::List { path, json } => commands::list::run(path.as_deref(), json, &config)?,
        Commands::Remove { path } => commands::remove::run(&path, config)?,
    }

    Ok(())
}
