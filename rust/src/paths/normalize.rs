pub fn normalize_workspace_relative(path: &str) -> String {
    let mut stack: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            if !stack.is_empty() {
                stack.pop();
            }
            continue;
        }
        stack.push(segment);
    }

    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_basic_paths() {
        assert_eq!(normalize_workspace_relative("a/b/c.md"), "a/b/c.md");
        assert_eq!(normalize_workspace_relative("/a/b/c.md"), "a/b/c.md");
        assert_eq!(normalize_workspace_relative("../a/b/c.md"), "a/b/c.md");
        assert_eq!(
            normalize_workspace_relative("./docs/foo.txt"),
            "docs/foo.txt"
        );
        assert_eq!(normalize_workspace_relative("../etc/passwd"), "etc/passwd");
        assert_eq!(normalize_workspace_relative("foo/../bar"), "bar");
    }

    #[test]
    fn normalize_root() {
        assert_eq!(normalize_workspace_relative(""), "");
        assert_eq!(normalize_workspace_relative("/"), "");
        assert_eq!(normalize_workspace_relative(".."), "");
        assert_eq!(normalize_workspace_relative("../.."), "");
    }
}
