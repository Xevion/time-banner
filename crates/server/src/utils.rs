//! General utility functions used across the application.

use http::HeaderMap;

/// Reads a header's value as a `&str`, treating missing or non-ASCII values
/// alike as absent.
pub trait HeaderMapExt {
    fn get_str(&self, name: &str) -> Option<&str>;
}

impl HeaderMapExt for HeaderMap {
    fn get_str(&self, name: &str) -> Option<&str> {
        self.get(name).and_then(|v| v.to_str().ok())
    }
}

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
    use assert2::check;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::simple("file.txt", Some(("file", "txt")))]
    #[case::nested_path("path/to/file.png", Some(("path/to/file", "png")))]
    #[case::no_extension("noextension", None)]
    #[case::dotfile(".dotfile", None)]
    #[case::trailing_dot("file.", Some(("file", "")))]
    #[case::multiple_dots("file.name.ext", Some(("file.name", "ext")))]
    fn splits_on_the_last_extension(#[case] path: &str, #[case] expected: Option<(&str, &str)>) {
        check!(split_on_extension(path) == expected);
    }

    #[rstest]
    #[case::simple("file.txt", ("file", "txt"))]
    #[case::nested_path("path/to/file.png", ("path/to/file", "png"))]
    #[case::no_extension_defaults_to_svg("noextension", ("noextension", "svg"))]
    #[case::dotfile_defaults_to_svg(".dotfile", (".dotfile", "svg"))]
    #[case::trailing_dot("file.", ("file", ""))]
    fn defaults_to_svg_when_no_extension(#[case] path: &str, #[case] expected: (&str, &str)) {
        check!(parse_path(path) == expected);
    }
}
