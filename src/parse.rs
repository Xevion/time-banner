use chrono::{DateTime, FixedOffset, Utc};

/// Split a path into a tuple of the preceding path and the extension.
/// Can handle paths with multiple dots (period characters).
/// Returns None if there is no extension.
/// Returns None if the preceding path is empty (for example, dotfiles like ".env").
pub fn split_on_extension(path: &str) -> Option<(&str, &str)> {
    let split = path.rsplit_once('.');
    if split.is_none() { return None; }

    // Check that the file is not a dotfile (.env)
    if split.unwrap().0.len() == 0 {
        return None;
    }

    Some(split.unwrap())
}

pub fn parse_absolute(raw: String) -> Result<(DateTime<Utc>, FixedOffset), String> {
    let datetime_with_offset = DateTime::parse_from_rfc3339(&raw);
    if datetime_with_offset.is_err() {
        return Err("Failed to parse datetime".to_string());
    }

    Ok((datetime_with_offset.unwrap().with_timezone(&Utc), *(datetime_with_offset.unwrap().offset())))
}