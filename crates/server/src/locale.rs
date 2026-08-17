//! `Accept-Language`/`?locale=` negotiation.
//!
//! `icu_locale` has no comma/`q=`-weighted header parser anywhere in its
//! family (nor does anything else current on crates.io), so ranking is
//! hand-rolled here; `icu_locale::Locale::try_from_str` supplies real BCP 47
//! parsing and validation for each candidate tag. Negotiation never errors:
//! an unparseable tag, an unsupported language, or an absent header all fall
//! through to `en`, per SPEC 7.

use icu_locale::Locale;
use time_banner_render::locale::SUPPORTED_LANGUAGE_CODES;

const DEFAULT_LOCALE: &str = "en";

/// Resolves the language to render words in. `?locale=` wins outright;
/// otherwise the highest-ranked supported candidate from `Accept-Language`.
pub fn resolve(query_locale: Option<&str>, accept_language: Option<&str>) -> String {
    if let Some(tag) = query_locale
        && let Some(code) = supported_primary_subtag(tag)
    {
        return code;
    }

    if let Some(header) = accept_language {
        for (_, tag) in ranked_candidates(header) {
            if let Some(code) = supported_primary_subtag(&tag) {
                return code;
            }
        }
    }

    DEFAULT_LOCALE.to_string()
}

/// Parses `tag` as BCP 47 and returns its primary language subtag if it's
/// one this service actually has words for.
fn supported_primary_subtag(tag: &str) -> Option<String> {
    let locale = Locale::try_from_str(tag).ok()?;
    let code = locale.id.language.as_str();
    SUPPORTED_LANGUAGE_CODES
        .contains(&code)
        .then(|| code.to_string())
}

/// Splits an `Accept-Language` header into `(q, tag)` pairs, ranked highest
/// first. A missing or unparseable `q` defaults to `1.0`. Ties keep the
/// header's own order, since RFC 7231 doesn't define tie-breaking.
fn ranked_candidates(header: &str) -> Vec<(f32, String)> {
    let mut candidates: Vec<(f32, String)> = header
        .split(',')
        .filter_map(|part| {
            let mut fields = part.trim().splitn(2, ';');
            let tag = fields.next()?.trim();
            if tag.is_empty() {
                return None;
            }
            let q = fields
                .next()
                .and_then(|rest| rest.trim().strip_prefix("q="))
                .and_then(|value| value.trim().parse::<f32>().ok())
                .unwrap_or(1.0);
            Some((q, tag.to_string()))
        })
        .collect();

    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    candidates
}

#[cfg(test)]
mod tests {
    use assert2::check;

    use super::*;

    #[test]
    fn query_wins_over_a_conflicting_header() {
        check!(resolve(Some("de"), Some("fr")) == "de");
    }

    #[test]
    fn header_alone_negotiates_the_supported_language() {
        check!(resolve(None, Some("fr")) == "fr");
    }

    #[test]
    fn higher_q_value_wins_over_header_order() {
        check!(resolve(None, Some("fr;q=0.5, de;q=0.9")) == "de");
    }

    #[test]
    fn a_malformed_candidate_is_skipped_in_favor_of_the_next() {
        check!(resolve(None, Some("not a bcp47 tag!!, de")) == "de");
    }

    #[test]
    fn an_unsupported_language_set_falls_back_to_english() {
        check!(resolve(None, Some("xx, yy;q=0.9")) == "en");
    }

    #[test]
    fn an_absent_header_falls_back_to_english() {
        check!(resolve(None, None) == "en");
    }

    #[test]
    fn an_unsupported_query_locale_falls_through_to_the_header() {
        check!(resolve(Some("xx"), Some("de")) == "de");
    }
}
