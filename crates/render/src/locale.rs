//! Selects a `timeago` translation by ISO 639-1 code.
//!
//! `timeago` bundles 19 languages by default (`isolang` and `translations`
//! are both default-on features). This module is the only place `isolang`
//! is used directly, so `server`'s negotiation only ever deals in plain
//! codes.

use timeago::BoxedLanguage;

/// ISO 639-1 codes for every language `timeago` bundles by default. This is
/// what `server`'s `Accept-Language`/`?locale=` negotiation checks
/// candidates against.
pub const SUPPORTED_LANGUAGE_CODES: &[&str] = &[
    "eu", "be", "zh", "da", "en", "fr", "de", "it", "ja", "ko", "pl", "pt", "ro", "ru", "es", "sv",
    "th", "tr", "uk",
];

/// Looks up the `timeago` translation for an ISO 639-1 code, if one is
/// bundled. `None` covers both an unrecognized code and a valid code with no
/// bundled translation.
pub fn language_for(code: &str) -> Option<BoxedLanguage> {
    // timeago bundles a Basque translation but its own `from_isolang`
    // dispatch table omits it, so it's reached directly here instead.
    if code == "eu" {
        return Some(timeago::languages::boxup(
            timeago::languages::basque::Basque,
        ));
    }
    isolang::Language::from_639_1(code).and_then(timeago::languages::from_isolang)
}

/// The fallback used when nothing was negotiated.
pub fn default_language() -> BoxedLanguage {
    timeago::languages::boxup(timeago::languages::english::English)
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use rstest::rstest;
    use std::time::Duration;

    use super::*;

    #[rstest]
    #[case::supported("fr")]
    #[case::default_language_itself("en")]
    fn resolves_a_supported_code(#[case] code: &str) {
        check!(language_for(code).is_some());
    }

    #[rstest]
    #[case::not_bundled_by_timeago("ht")] // Haitian Creole: valid ISO 639-1, no timeago translation
    #[case::not_a_language_code("xx")]
    #[case::empty("")]
    fn rejects_an_unsupported_code(#[case] code: &str) {
        check!(language_for(code).is_none());
    }

    #[test]
    fn french_translation_produces_french_words() {
        let formatter = timeago::Formatter::with_language(language_for("fr").unwrap());
        let text = formatter.convert(Duration::from_secs(3600));
        check!(text.contains("heure"));
    }

    #[test]
    fn default_language_produces_english_words() {
        let formatter = timeago::Formatter::with_language(default_language());
        let text = formatter.convert(Duration::from_secs(3600));
        check!(text.contains("hour"));
    }

    #[test]
    fn every_supported_code_actually_resolves() {
        let unresolved: Vec<&str> = SUPPORTED_LANGUAGE_CODES
            .iter()
            .filter(|code| language_for(code).is_none())
            .copied()
            .collect();
        check!(unresolved == Vec::<&str>::new());
    }
}
