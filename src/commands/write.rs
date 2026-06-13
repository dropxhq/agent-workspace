use std::io::{self, Read};

use crate::storage::{BackendHandle, WorkspaceBackend};
use crate::ranges::{parse_ranges, LineRange};
use crate::error::{WsError, WsResult};

pub fn run(
    path: &str,
    ranges: Option<&str>,
    created_by: &str,
    desc: &str,
    content_arg: Option<&str>,
    backend: &BackendHandle,
) -> WsResult<()> {
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

    backend.write(path, parsed_range.as_ref(), &new_content, created_by, desc)?;

    Ok(())
}

fn read_input(content_arg: Option<&str>) -> WsResult<String> {
    if let Some(content) = content_arg {
        return Ok(content.to_string());
    }

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).map_err(WsError::Io)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use crate::ranges::{apply_write_ranges, LineRange};

    #[test]
    fn write_range_semantics() {
        let existing = "a\nb\nc\n";
        let r = LineRange { start: 2, end: 2 };
        assert_eq!(apply_write_ranges(existing, &r, "B\n"), "a\nB\nc\n");
    }
}
