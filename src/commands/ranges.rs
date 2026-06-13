use crate::error::{WsError, WsResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

pub fn parse_ranges(input: &str) -> WsResult<Vec<LineRange>> {
    let mut ranges = Vec::new();
    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (start, end) = if let Some((a, b)) = part.split_once('-') {
            (a.trim(), b.trim())
        } else {
            (part, part)
        };

        let start: usize = start
            .parse()
            .map_err(|_| WsError::InvalidRanges(format!("invalid start in '{part}'")))?;
        let end: usize = end
            .parse()
            .map_err(|_| WsError::InvalidRanges(format!("invalid end in '{part}'")))?;

        if start == 0 || end == 0 {
            return Err(WsError::InvalidRanges(
                "line numbers are 1-indexed and must be >= 1".to_string(),
            ));
        }
        if start > end {
            return Err(WsError::InvalidRanges(format!(
                "start ({start}) must be <= end ({end})"
            )));
        }

        ranges.push(LineRange { start, end });
    }

    if ranges.is_empty() {
        return Err(WsError::InvalidRanges(
            "no valid ranges specified".to_string(),
        ));
    }

    ranges.sort_by_key(|r| r.start);
    Ok(ranges)
}

pub fn line_in_ranges(line_no: usize, ranges: &[LineRange]) -> bool {
    ranges
        .iter()
        .any(|r| line_no >= r.start && line_no <= r.end)
}

pub fn apply_write_ranges(existing: &str, ranges: &LineRange, new_content: &str) -> String {
    let lines: Vec<&str> = if existing.is_empty() {
        Vec::new()
    } else {
        existing.split_inclusive('\n').collect()
    };

    let mut result = String::new();

    for (idx, line) in lines.iter().enumerate() {
        let line_no = idx + 1;
        if line_no < ranges.start {
            result.push_str(line);
        } else if line_no > ranges.end {
            result.push_str(line);
        }
    }

    let insert_at = lines
        .iter()
        .take(ranges.start.saturating_sub(1))
        .map(|l| l.len())
        .sum::<usize>();

    result.insert_str(insert_at, new_content);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_and_multiple_ranges() {
        let r = parse_ranges("1-10").unwrap();
        assert_eq!(r, vec![LineRange { start: 1, end: 10 }]);

        let r = parse_ranges("1-10,20-30").unwrap();
        assert_eq!(
            r,
            vec![
                LineRange { start: 1, end: 10 },
                LineRange { start: 20, end: 30 }
            ]
        );
    }

    #[test]
    fn parse_single_line() {
        let r = parse_ranges("5").unwrap();
        assert_eq!(r, vec![LineRange { start: 5, end: 5 }]);
    }

    #[test]
    fn apply_write_ranges_replaces_middle() {
        let existing = "line1\nline2\nline3\n";
        let ranges = LineRange { start: 2, end: 2 };
        let result = apply_write_ranges(existing, &ranges, "replaced\n");
        assert_eq!(result, "line1\nreplaced\nline3\n");
    }

    #[test]
    fn apply_write_ranges_full_replace() {
        let existing = "a\nb\nc\n";
        let ranges = LineRange { start: 1, end: 3 };
        let result = apply_write_ranges(existing, &ranges, "x\n");
        assert_eq!(result, "x\n");
    }
}
