use crate::storage::{BackendHandle, WorkspaceBackend};
use crate::ranges::{parse_ranges, LineRange};
use crate::error::{WsError, WsResult};

pub fn run(
    path: &str,
    ranges: Option<&str>,
    created_by: &str,
    desc: &str,
    content: &str,
    backend: &BackendHandle,
) -> WsResult<()> {
    let new_content = content.to_string();

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
