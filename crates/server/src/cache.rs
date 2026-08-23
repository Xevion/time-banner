//! Freshness and validators for rendered banners (SPEC.md section 9).
//!
//! The useful property of a time banner is that its content changes on a
//! schedule knowable in advance: a badge reading `2 hours ago` does not change
//! until it reads `3 hours ago`. Both the `ETag` and the `max-age` here come
//! from that display value rather than from a guessed TTL, so a banner's cache
//! expires at the moment it becomes wrong.

use axum::http::{HeaderMap, header};
use jiff::{SignedDuration, Timestamp};
use sha2::{Digest, Sha256};
use time_banner_render::RenderContext;

use crate::utils::HeaderMapExt;

/// Longest freshness the service claims, and the horizon the change search
/// gives up at. A display value that has not moved a year out is treated as
/// one that never will.
const MAX_AGE_CAP: i64 = 31_536_000;

/// Coarsest probe step. `max-age` is whole seconds, so a value changing faster
/// than this is reported as changing every second either way.
const MIN_STEP: SignedDuration = SignedDuration::from_secs(1);

/// What a display value looks like had the request arrived at some other
/// instant. `None` covers an instant the request could not be served at,
/// which the search treats as a change so that it shortens freshness rather
/// than extending it.
pub type Display<'a> = dyn Fn(Timestamp) -> Option<String> + 'a;

/// What a response says about its own lifetime.
pub struct Freshness {
    directive: String,
    /// Instant the current display value took effect, for `Last-Modified`.
    /// Absent where the value has no start, which is every fixed response.
    pub since: Option<Timestamp>,
}

impl Freshness {
    /// A response fully determined by its request: it will never read
    /// differently, so it can be held for as long as anything is held.
    pub fn fixed() -> Self {
        Freshness {
            directive: format!("public, max-age={MAX_AGE_CAP}, immutable"),
            since: None,
        }
    }

    /// The `Cache-Control` value, before `resolve` prefixes `private` onto
    /// geolocated responses.
    pub fn directive(&self) -> &str {
        &self.directive
    }
}

/// Freshness for a response that goes stale when its text next changes.
///
/// Staleness is cosmetic for every mode drawn this way, so a cache is invited
/// to keep serving the old bytes while it fetches new ones.
pub fn until_change(display: &Display<'_>, now: Timestamp, current: &str) -> Freshness {
    // Truncating toward zero under-claims by less than a second, which is the
    // safe direction; a change already due is still reported as one second.
    let max_age = next_change(display, now, current)
        .map_or(MAX_AGE_CAP, |at| at.duration_since(now).as_secs())
        .clamp(1, MAX_AGE_CAP);

    Freshness {
        directive: format!("public, max-age={max_age}, stale-while-revalidate={max_age}"),
        since: last_change(display, now, current),
    }
}

/// Evaluates `display` at `now + offset`.
fn probe(display: &Display<'_>, now: Timestamp, offset: SignedDuration) -> Option<String> {
    now.checked_add(offset).ok().and_then(display)
}

/// The earliest instant after `now` at which `display` stops returning
/// `current`, or `None` if it still agrees at the horizon.
///
/// Doubling from one second brackets the first change, and a bisection inside
/// that bracket pins it exactly. This assumes a display value holds steady and
/// then moves on without ever coming back, which is true of every mode here:
/// `timeago`'s buckets only advance, and a time pattern only counts upward. A
/// value that cycled faster than the first step would be bracketed at one
/// second instead, which under-claims freshness rather than over-claiming it.
fn next_change(display: &Display<'_>, now: Timestamp, current: &str) -> Option<Timestamp> {
    let horizon = SignedDuration::from_secs(MAX_AGE_CAP);
    let same =
        |offset: SignedDuration| probe(display, now, offset).is_some_and(|text| text == current);

    let mut lo = SignedDuration::ZERO;
    let mut hi = MIN_STEP;
    while same(hi) {
        if hi >= horizon {
            return None;
        }
        lo = hi;
        hi = hi.checked_mul(2).unwrap_or(horizon).min(horizon);
    }

    while let Some(mid) = midpoint(lo, hi) {
        if same(mid) { lo = mid } else { hi = mid }
    }
    now.checked_add(hi).ok()
}

/// The instant the current display value took effect, mirroring
/// [`next_change`] backwards. `None` where it was already current a year ago.
fn last_change(display: &Display<'_>, now: Timestamp, current: &str) -> Option<Timestamp> {
    let horizon = SignedDuration::from_secs(-MAX_AGE_CAP);
    let same =
        |offset: SignedDuration| probe(display, now, offset).is_some_and(|text| text == current);

    let mut lo = SignedDuration::ZERO;
    let mut hi = -MIN_STEP;
    while same(hi) {
        if hi <= horizon {
            return None;
        }
        lo = hi;
        hi = hi.checked_mul(2).unwrap_or(horizon).max(horizon);
    }

    while let Some(mid) = midpoint(hi, lo) {
        if same(mid) { lo = mid } else { hi = mid }
    }
    now.checked_add(lo).ok()
}

/// The instant halfway between two offsets, or `None` once they are adjacent
/// and there is nothing left to bisect.
fn midpoint(lo: SignedDuration, hi: SignedDuration) -> Option<SignedDuration> {
    let gap = (hi - lo).checked_div(2)?;
    if gap.is_zero() { None } else { Some(lo + gap) }
}

/// Strong validator over the display value and every input that changes the
/// bytes.
///
/// The request instant is deliberately absent: two requests an hour apart that
/// draw the same string are the same representation, which is the whole point.
/// The crate version and the face's own checksum are folded in so a redeploy
/// or a rebuilt font bundle invalidates rather than serving old bytes under a
/// tag that still matches.
pub fn etag(context: &RenderContext, display: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [
        env!("CARGO_PKG_VERSION"),
        context.family().manifest().subset_sha256,
        &format!("{:?}", context.output_form),
        &format!("{:?}", context.output_format),
        &format!("{:?}", context.text_mode),
        display,
    ] {
        // Length-prefixed so that no two different input tuples can
        // concatenate into the same byte string.
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }

    let digest = hasher.finalize();
    let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
    format!("\"{hex}\"")
}

/// Whether `If-None-Match` says the client already holds this entity.
///
/// Comparison is weak, per RFC 9110: a `W/` prefix on either side does not
/// prevent a match, since both describe the same display value.
pub fn matches_if_none_match(headers: &HeaderMap, etag: &str) -> bool {
    let Some(raw) = headers.get_str(header::IF_NONE_MATCH.as_str()) else {
        return false;
    };
    if raw.trim() == "*" {
        return true;
    }

    let opaque = |tag: &str| tag.trim().trim_start_matches("W/").trim().to_string();
    let held = opaque(etag);
    raw.split(',').any(|candidate| opaque(candidate) == held)
}

/// Whether `If-Modified-Since` says the client's copy is still current.
///
/// Only consulted when there is no `If-None-Match`, which RFC 9110 makes the
/// stronger signal.
pub fn matches_if_modified_since(headers: &HeaderMap, since: Option<Timestamp>) -> bool {
    let (Some(raw), Some(since)) = (headers.get_str(header::IF_MODIFIED_SINCE.as_str()), since)
    else {
        return false;
    };

    jiff::fmt::rfc2822::parse(raw)
        .is_ok_and(|held| since.as_second() <= held.timestamp().as_second())
}

/// Formats an instant as an HTTP-date, the only form RFC 9110 requires a
/// recipient to produce.
pub fn http_date(at: Timestamp) -> String {
    at.to_zoned(jiff::tz::TimeZone::UTC)
        .strftime("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

#[cfg(test)]
mod tests {
    use assert2::check;
    use jiff::tz::TimeZone;
    use rstest::rstest;
    use time_banner_render::{OutputForm, OutputFormat, TextMode};

    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn context(form: OutputForm, value: Timestamp) -> RenderContext {
        RenderContext {
            value,
            output_form: form,
            output_format: OutputFormat::Svg,
            tz: TimeZone::UTC,
            now: Timestamp::from_second(NOW).unwrap(),
            format: None,
            locale: None,
            font: None,
            text_mode: TextMode::default(),
        }
    }

    /// The display value as a function of when the request arrives, which is
    /// the shape the search consumes.
    fn display(base: &RenderContext) -> impl Fn(Timestamp) -> Option<String> + '_ {
        move |instant| {
            RenderContext {
                now: instant,
                ..base.clone()
            }
            .display_text()
            .ok()
            .flatten()
        }
    }

    /// The whole design rests on this: a `max-age` is only honest if it lands
    /// exactly on the instant the wording changes. One nanosecond earlier must
    /// still read the same, and the boundary itself must not.
    #[rstest]
    #[case::seconds(5)]
    #[case::a_minute_in(60)]
    #[case::minutes(400)]
    #[case::an_hour_in(3600)]
    #[case::hours(20_000)]
    #[case::days(300_000)]
    #[case::months(9_000_000)]
    #[case::in_the_future(-4000)]
    fn the_boundary_is_where_the_wording_changes(#[case] age_secs: i64) {
        let now = Timestamp::from_second(NOW).unwrap();
        let base = context(
            OutputForm::Relative,
            now - SignedDuration::from_secs(age_secs),
        );
        let at = display(&base);
        let current = at(now).expect("relative output always draws text");

        let boundary = next_change(&at, now, &current).expect("relative wording always moves on");
        check!(boundary > now);
        check!(at(boundary - SignedDuration::from_nanos(1)).as_deref() == Some(&*current));
        check!(at(boundary).as_deref() != Some(&*current));
    }

    /// `Last-Modified` is the same search run backwards, and has to be just as
    /// exact or it claims the value changed at a moment it did not.
    #[rstest]
    #[case::minutes(400)]
    #[case::an_hour_in(3600)]
    #[case::days(300_000)]
    fn the_start_instant_is_where_the_wording_arrived(#[case] age_secs: i64) {
        let now = Timestamp::from_second(NOW).unwrap();
        let base = context(
            OutputForm::Relative,
            now - SignedDuration::from_secs(age_secs),
        );
        let at = display(&base);
        let current = at(now).expect("relative output always draws text");

        let started = last_change(&at, now, &current).expect("the wording arrived at some point");
        check!(started <= now);
        check!(at(started).as_deref() == Some(&*current));
        check!(at(started - SignedDuration::from_nanos(1)).as_deref() != Some(&*current));
    }

    /// Absolute output of an instant ignores the clock entirely, so the search
    /// has to report no change rather than inventing one at the horizon.
    #[test]
    fn a_fixed_instant_never_changes() {
        let now = Timestamp::from_second(NOW).unwrap();
        let base = context(
            OutputForm::Absolute,
            Timestamp::from_second(946_684_800).unwrap(),
        );
        let at = display(&base);
        let current = at(now).unwrap();

        check!(next_change(&at, now, &current) == None);
        check!(last_change(&at, now, &current) == None);
    }

    /// A badge just short of the next wording change should say so, rather
    /// than rounding up into a minute of being wrong.
    #[test]
    fn max_age_runs_out_when_the_badge_does() {
        let now = Timestamp::from_second(NOW).unwrap();
        // 119 seconds old reads "1 minute ago" and turns over in 1 second.
        let base = context(OutputForm::Relative, now - SignedDuration::from_secs(119));
        let at = display(&base);
        let current = at(now).unwrap();

        let freshness = until_change(&at, now, &current);
        check!(freshness.directive().contains("max-age=1,"));
        check!(freshness.directive().starts_with("public,"));
    }

    #[test]
    fn a_fixed_response_is_immutable() {
        check!(Freshness::fixed().directive().contains("immutable"));
        check!(Freshness::fixed().since == None);
    }

    #[rstest]
    #[case::exact("\"abc\"", true)]
    #[case::weak_on_the_wire("W/\"abc\"", true)]
    #[case::in_a_list("\"nope\", \"abc\"", true)]
    #[case::wildcard("*", true)]
    #[case::different("\"nope\"", false)]
    fn if_none_match_compares_weakly(#[case] sent: &str, #[case] expected: bool) {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, sent.parse().unwrap());
        check!(matches_if_none_match(&headers, "\"abc\"") == expected);
    }

    #[test]
    fn if_none_match_is_absent_by_default() {
        check!(!matches_if_none_match(&HeaderMap::new(), "\"abc\""));
    }

    /// The date has to survive the round trip, or a conditional request that
    /// echoes our own `Last-Modified` back would still be answered with a
    /// full render.
    #[test]
    fn a_formatted_date_is_accepted_back() {
        let at = Timestamp::from_second(NOW).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MODIFIED_SINCE, http_date(at).parse().unwrap());

        check!(http_date(at) == "Tue, 14 Nov 2023 22:13:20 GMT");
        check!(matches_if_modified_since(&headers, Some(at)));
        check!(!matches_if_modified_since(
            &headers,
            Some(at + SignedDuration::from_secs(1))
        ));
        check!(!matches_if_modified_since(&headers, None));
    }
}
