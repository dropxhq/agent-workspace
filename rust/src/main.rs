use std::process;

fn main() {
    if let Err(e) = agent_workspace::cli::run() {
        eprintln!("error: {e}");
        process::exit(e.exit_code().into());
    }
}
