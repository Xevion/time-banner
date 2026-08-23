use crate::cache::{self, Freshness};
use crate::client_ip::ClientIp;
use crate::error::TimeBannerError;
use crate::resolve::Resolution;
use crate::utils::parse_path;
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use time_banner_core::{Anchor, parse_time_value, parse_time_value_anchored};
use time_banner_render::{
    OutputForm, OutputFormat, RenderContext, Rendered, convert_png_to_ico,
    generate_favicon_png_bytes, template::render_index_page,
};

/// Renders a time value, moving actual rasterization off the async executor
/// for PNG output. SVG output never rasterizes, so it stays inline.
async fn render_time_async(context: RenderContext) -> Result<Rendered, TimeBannerError> {
    match context.output_format {
        OutputFormat::Svg => context.render().map_err(TimeBannerError::from),
        OutputFormat::Png => tokio::task::spawn_blocking(move || context.render())
            .await
            .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
            .map_err(TimeBannerError::from),
    }
}

/// Generates the favicon clock PNG off the async executor.
async fn generate_favicon_png_bytes_async(
    time: Timestamp,
    tz: TimeZone,
) -> Result<Vec<u8>, TimeBannerError> {
    tokio::task::spawn_blocking(move || generate_favicon_png_bytes(time, tz))
        .await
        .map_err(|e| TimeBannerError::Internal(format!("render task panicked: {}", e)))?
        .map_err(TimeBannerError::from)
}

/// Renders one banner, shared by every mode that draws a single time value.
async fn banner(
    path: String,
    output_form: OutputForm,
    resolution: Resolution,
    request: HeaderMap,
) -> Result<Response, TimeBannerError> {
    let (raw_time, extension) = parse_path(&path);
    let output_format = OutputFormat::from(extension);
    let mime_type = output_format.mime_type();

    // Only `relative` renders words today; `Content-Language` and
    // `Vary: Accept-Language` are scoped to responses that actually depend
    // on the negotiated locale, unlike `Timezone`, which SPEC 6 requires on
    // every response regardless of mode.
    let renders_words = matches!(output_form, OutputForm::Relative);
    let locale = resolution.locale.clone();

    let (value, anchor) = parse_time_value_anchored(raw_time, resolution.now, &resolution.tz)?;
    let context = RenderContext {
        value,
        output_form,
        output_format,
        tz: resolution.tz.clone(),
        now: resolution.now,
        format: resolution.format.clone(),
        locale: Some(locale.clone()),
        font: resolution.font,
        text_mode: resolution.text_mode,
    };

    let display = context.display_text()?.unwrap_or_default();
    let etag = cache::etag(&context, &display);

    // Nothing the wall clock does can change this response: either the caller
    // pinned the reference instant, or the value names an instant outright and
    // absolute output of a fixed instant reads the same forever.
    let fixed = resolution.now_pinned
        || (matches!(output_form, OutputForm::Absolute) && anchor == Anchor::Fixed);
    let freshness = if fixed {
        Freshness::fixed()
    } else {
        // Re-parsed per probe, so a value written relative to `now` follows the
        // instant being asked about instead of staying where it first resolved.
        let at = |instant: Timestamp| {
            let value = parse_time_value(raw_time, instant, &resolution.tz).ok()?;
            RenderContext {
                value,
                now: instant,
                ..context.clone()
            }
            .display_text()
            .ok()
            .flatten()
        };
        cache::until_change(&at, resolution.now, &display)
    };

    let mut headers = HeaderMap::new();
    if let Ok(value) = HeaderValue::from_str(&etag) {
        headers.insert(header::ETAG, value);
    }
    if let Ok(value) = HeaderValue::from_str(freshness.directive()) {
        headers.insert(header::CACHE_CONTROL, value);
    }
    if let Some(since) = freshness.since
        && let Ok(value) = HeaderValue::from_str(&cache::http_date(since))
    {
        headers.insert(header::LAST_MODIFIED, value);
    }
    if renders_words && let Ok(value) = HeaderValue::from_str(&locale) {
        headers.insert(header::CONTENT_LANGUAGE, value);
        headers.append(header::VARY, HeaderValue::from_static("Accept-Language"));
    }

    // RFC 9110 makes the entity tag the stronger signal, so a request carrying
    // both is answered on `If-None-Match` alone.
    let already_held = if request.contains_key(header::IF_NONE_MATCH) {
        cache::matches_if_none_match(&request, &etag)
    } else {
        cache::matches_if_modified_since(&request, freshness.since)
    };
    // Returned with no body part at all rather than an empty one, so that
    // nothing describes a content type. A cache updates the response it holds
    // with the headers a `304` carries, and a placeholder type here would
    // overwrite the real one it already has.
    if already_held {
        return Ok((StatusCode::NOT_MODIFIED, headers).into_response());
    }

    let rendered = render_time_async(context).await?;
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime_type));

    // More than one entry means the requested face didn't cover the string and
    // something else drew it, which is otherwise invisible to the caller. It
    // follows from `?font=` and the text, both already covered by the ETag, so
    // there is no request header for a cache to vary on.
    if let Some(font) = rendered.font.as_deref()
        && let Ok(value) = HeaderValue::from_str(font)
    {
        headers.insert("font", value);
    }

    Ok((StatusCode::OK, headers, rendered.bytes).into_response())
}

/// Root handler - renders a minimal demo page with live examples and usage docs.
pub async fn index_handler() -> Result<impl IntoResponse, TimeBannerError> {
    let html = render_index_page(Timestamp::now())
        .map_err(|e| TimeBannerError::RenderError(format!("Failed to render index page: {}", e)))?;
    Ok(Html(html))
}

/// Handles `/relative/{time}` - displays time in relative format ("2 hours ago").
pub async fn relative_handler(
    Path(path): Path<String>,
    resolution: Resolution,
    request: HeaderMap,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Relative time request");
    banner(path, OutputForm::Relative, resolution, request).await
}

/// Handles `/absolute/{time}` - displays time in absolute format ("2025-01-17 14:30:00 UTC").
pub async fn absolute_handler(
    Path(path): Path<String>,
    resolution: Resolution,
    request: HeaderMap,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Absolute time request");
    banner(path, OutputForm::Absolute, resolution, request).await
}

/// Handles `/{time}` - implicit absolute time display (same as absolute_handler).
pub async fn implicit_handler(
    Path(path): Path<String>,
    resolution: Resolution,
    request: HeaderMap,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::debug!(path, "Implicit time request");
    banner(path, OutputForm::Absolute, resolution, request).await
}

/// Handles `/favicon.ico` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_handler(
    ClientIp(addr): ClientIp,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon request");

    let png_bytes = generate_favicon_png_bytes_async(resolution.now, resolution.tz).await?;
    let ico_bytes = convert_png_to_ico(&png_bytes)?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/x-icon"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        ico_bytes,
    ))
}

/// Handles `/favicon.png` - generates a dynamic clock favicon showing the current time.
pub async fn favicon_png_handler(
    ClientIp(addr): ClientIp,
    resolution: Resolution,
) -> Result<impl IntoResponse, TimeBannerError> {
    tracing::info!(client_ip = %addr, "Favicon PNG request");

    let png_bytes = generate_favicon_png_bytes_async(resolution.now, resolution.tz).await?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        png_bytes,
    ))
}

/// Fallback handler for unmatched routes.
pub async fn fallback_handler() -> TimeBannerError {
    TimeBannerError::NotFound
}
