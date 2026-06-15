pub fn is_metadata_path(relative: &str, metadata_suffix: &str) -> bool {
    relative.ends_with(metadata_suffix)
}

pub fn metadata_path_for(relative: &str, metadata_suffix: &str) -> String {
    format!("{relative}{metadata_suffix}")
}

pub fn data_path_from_metadata(relative: &str, metadata_suffix: &str) -> Option<String> {
    relative
        .strip_suffix(metadata_suffix)
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_path_detection() {
        assert!(is_metadata_path("foo.txt.meta.yaml", ".meta.yaml"));
        assert!(!is_metadata_path("foo.txt", ".meta.yaml"));
        assert_eq!(
            metadata_path_for("docs/foo.txt", ".meta.yaml"),
            "docs/foo.txt.meta.yaml"
        );
        assert_eq!(
            data_path_from_metadata("docs/foo.txt.meta.yaml", ".meta.yaml"),
            Some("docs/foo.txt".to_string())
        );
    }
}
