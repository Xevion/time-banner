//! General utility functions used across the application.

/// Splits a path on the last dot to extract filename and extension.
/// Returns None for dotfiles (paths starting with a dot).
pub fn split_on_extension(path: &str) -> Option<(&str, &str)> {
    let split = path.rsplit_once('.')?;

    // Check that the file is not a dotfile (.env)
    if split.0.is_empty() {
        return None;
    }

    Some(split)
}

/// Parses path into (filename, extension). Defaults to "svg" if no extension found.
pub fn parse_path(path: &str) -> (&str, &str) {
    split_on_extension(path).unwrap_or((path, "svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_on_extension() {
        assert_eq!(split_on_extension("file.txt"), Some(("file", "txt")));
        assert_eq!(
            split_on_extension("path/to/file.png"),
            Some(("path/to/file", "png"))
        );
        assert_eq!(split_on_extension("noextension"), None);
        assert_eq!(split_on_extension(".dotfile"), None); // dotfiles return None
        assert_eq!(split_on_extension("file."), Some(("file", "")));
        assert_eq!(
            split_on_extension("file.name.ext"),
            Some(("file.name", "ext"))
        );
    }

    #[test]
    fn test_parse_path() {
        assert_eq!(parse_path("file.txt"), ("file", "txt"));
        assert_eq!(parse_path("path/to/file.png"), ("path/to/file", "png"));
        assert_eq!(parse_path("noextension"), ("noextension", "svg")); // default to svg
        assert_eq!(parse_path(".dotfile"), (".dotfile", "svg")); // dotfiles get svg default
        assert_eq!(parse_path("file."), ("file", ""));
    }
}
