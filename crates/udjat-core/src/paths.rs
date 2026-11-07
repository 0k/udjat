/// Check if a tool's file_path argument matches the target path.
///
/// - If `tool_path` is absolute: exact string equality with `target_path`
/// - If `tool_path` is relative: check if `target_path` ends with `tool_path`
///   using path component comparison (not string suffix)
pub fn file_path_matches(tool_path: &str, target_path: &str) -> bool {
    let tool = std::path::Path::new(tool_path);
    if tool.is_absolute() {
        tool_path == target_path
    } else {
        // Component-based suffix match: target must end with all components of tool_path
        let target_components: Vec<_> = std::path::Path::new(target_path).components().collect();
        let tool_components: Vec<_> = tool.components().collect();
        if tool_components.is_empty() || tool_components.len() > target_components.len() {
            return false;
        }
        let offset = target_components.len() - tool_components.len();
        target_components[offset..] == tool_components[..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_is_exact() {
        assert!(file_path_matches(
            "/home/user/src/main.rs",
            "/home/user/src/main.rs"
        ));
        assert!(!file_path_matches(
            "/home/user/src/main.rs",
            "/home/user/src/lib.rs"
        ));
    }

    #[test]
    fn relative_is_component_suffix() {
        assert!(file_path_matches(
            "src/main.rs",
            "/home/user/project/src/main.rs"
        ));
        assert!(file_path_matches(
            "main.rs",
            "/home/user/project/src/main.rs"
        ));
        assert!(!file_path_matches(
            "src/lib.rs",
            "/home/user/project/src/main.rs"
        ));
        // "rs" is not the component "src".
        assert!(!file_path_matches(
            "rs/main.rs",
            "/home/user/project/src/main.rs"
        ));
    }

    #[test]
    fn empty_tool_path_never_matches() {
        assert!(!file_path_matches("", "/home/user/src/main.rs"));
    }
}
