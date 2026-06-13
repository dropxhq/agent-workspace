use crate::commands::ranges::{line_in_ranges, LineRange};

pub fn filter_lines(content: &str, ranges: &[LineRange]) -> String {
    let mut filtered = String::new();
    for (idx, line) in content.split_inclusive('\n').enumerate() {
        let line_no = idx + 1;
        if line_in_ranges(line_no, ranges) {
            filtered.push_str(line);
        }
    }
    filtered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_only_requested_ranges() {
        let content = "one\ntwo\nthree\n";
        let ranges = [LineRange { start: 2, end: 3 }];
        assert_eq!(filter_lines(content, &ranges), "two\nthree\n");
    }
}
