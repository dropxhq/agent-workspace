use std::io::{self, Write};

use crate::backend::{BackendHandle, WorkspaceBackend};
use crate::commands::ranges::{line_in_ranges, parse_ranges};
use crate::error::{WsError, WsResult};

pub fn run(path: &str, ranges: Option<&str>, human: bool, backend: &BackendHandle) -> WsResult<()> {
    let parsed_ranges = ranges.map(parse_ranges).transpose()?;
    let content = backend.read(path, parsed_ranges.as_deref())?;

    if human {
        print_human(path, &content, parsed_ranges.as_deref())?;
    } else if let Some(ranges) = parsed_ranges.as_deref() {
        print_raw_filtered(&content, ranges)?;
    } else {
        print!("{content}");
    }

    Ok(())
}

fn print_human(
    relative: &str,
    content: &str,
    ranges: Option<&[crate::commands::ranges::LineRange]>,
) -> WsResult<()> {
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

fn print_raw_filtered(
    content: &str,
    ranges: &[crate::commands::ranges::LineRange],
) -> WsResult<()> {
    let mut stdout = io::stdout().lock();
    for (idx, line) in content.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
        if line_in_ranges(line_no, ranges) {
            write!(stdout, "{line}").map_err(WsError::Io)?;
        }
    }
    Ok(())
}
