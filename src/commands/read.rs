use std::fs;
use std::io::{self, Write};

use crate::commands::ranges::{line_in_ranges, parse_ranges};
use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::lock::FileLock;
use crate::workspace::{is_metadata_path, parse_ws_path};

pub fn run(
    path: &str,
    ranges: Option<&str>,
    human: bool,
    config: &Config,
) -> WsResult<()> {
    let resolved = parse_ws_path(path, config)?;

    if is_metadata_path(&resolved.relative, &config.metadata_suffix) {
        return Err(WsError::NotFound(resolved.relative));
    }

    if !resolved.absolute.is_file() {
        return Err(WsError::NotFound(resolved.relative));
    }

    let _lock = FileLock::shared(&resolved.absolute)?;

    let content = fs::read_to_string(&resolved.absolute).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            WsError::NotFound(resolved.relative.clone())
        } else {
            WsError::Io(e)
        }
    })?;

    let parsed_ranges = ranges.map(parse_ranges).transpose()?;

    if human {
        print_human(&resolved.relative, &content, parsed_ranges.as_deref())?;
    } else if let Some(ranges) = parsed_ranges.as_deref() {
        print_raw_filtered(&content, ranges)?;
    } else {
        print!("{content}");
    }

    Ok(())
}

fn print_human(relative: &str, content: &str, ranges: Option<&[crate::commands::ranges::LineRange]>) -> WsResult<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{relative}").map_err(WsError::Io)?;

    for (idx, line) in content.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
        if let Some(ranges) = ranges {
            if !line_in_ranges(line_no, ranges) {
                continue;
            }
        }
        let display = line.strip_suffix('\n').unwrap_or(line);
        writeln!(stdout, "{line_no:6} | {display}").map_err(WsError::Io)?;
    }

    Ok(())
}

fn print_raw_filtered(content: &str, ranges: &[crate::commands::ranges::LineRange]) -> WsResult<()> {
    let mut stdout = io::stdout().lock();
    for (idx, line) in content.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
        if line_in_ranges(line_no, ranges) {
            write!(stdout, "{line}").map_err(WsError::Io)?;
        }
    }
    Ok(())
}
