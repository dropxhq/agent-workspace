use std::fs;
use std::io::{self, Read};

use crate::commands::ranges::{apply_write_ranges, parse_ranges, LineRange};
use crate::config::Config;
use crate::error::{WsError, WsResult};
use crate::lock::FileLock;
use crate::meta::{build_metadata, sidecar_absolute};
use crate::workspace::parse_ws_path_for_write;

pub fn run(
    path: &str,
    ranges: Option<&str>,
    created_by: &str,
    desc: &str,
    content_arg: Option<&str>,
    config: &Config,
) -> WsResult<()> {
    let resolved = parse_ws_path_for_write(path, config)?;

    let new_content = read_input(content_arg)?;

    let parsed_range = ranges.map(parse_ranges).transpose()?.and_then(|mut v| {
        if v.len() > 1 {
            Some(Err(WsError::InvalidRanges(
                "write supports only a single range (START-END)".to_string(),
            )))
        } else {
            v.pop().map(Ok)
        }
    });
    if let Some(Err(e)) = parsed_range {
        return Err(e);
    }
    let parsed_range: Option<LineRange> = parsed_range.transpose()?;

    let _lock = FileLock::exclusive(&resolved.absolute)?;

    let final_content = if let Some(range) = &parsed_range {
        let existing = if resolved.absolute.is_file() {
            fs::read_to_string(&resolved.absolute).map_err(WsError::Io)?
        } else {
            String::new()
        };
        apply_write_ranges(&existing, range, &new_content)
    } else {
        new_content
    };

    if let Some(parent) = resolved.absolute.parent() {
        fs::create_dir_all(parent).map_err(WsError::Io)?;
    }

    fs::write(&resolved.absolute, &final_content).map_err(WsError::Io)?;

    let metadata = build_metadata(
        config,
        &resolved.relative,
        final_content.as_bytes(),
        created_by,
        desc,
    )?;

    let sidecar = sidecar_absolute(config, &resolved.relative)?;
    if let Some(parent) = sidecar.parent() {
        fs::create_dir_all(parent).map_err(WsError::Io)?;
    }
    metadata.write_to_sidecar(&sidecar)?;

    Ok(())
}

fn read_input(content_arg: Option<&str>) -> WsResult<String> {
    if let Some(content) = content_arg {
        return Ok(content.to_string());
    }

    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(WsError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use crate::commands::ranges::{apply_write_ranges, LineRange};

    #[test]
    fn write_range_semantics() {
        let existing = "a\nb\nc\n";
        let r = LineRange { start: 2, end: 2 };
        assert_eq!(apply_write_ranges(existing, &r, "B\n"), "a\nB\nc\n");
    }
}
