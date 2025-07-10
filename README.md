# time-banner

My first Rust project, intended to offer a simple way to display the current time relative, in an image format.

## Features

- [ ] Dynamic light/dark mode
  - [ ] Via Query Parameters for Raster or SVG
  - [ ] Via CSS for SVG
- [x] Relative or Absolute Format
- [ ] Dynamic Formats (currently basic template only)
- [ ] Caching Abilities
  - [ ] Relative caching for up to 59 seconds, purged on the minute
  - [ ] Absolute caching for up to 50MB, purged on an LRU basis
- [x] Flexible & Dynamic Browser API
  - [x] Basic routing with multiple formats
  - [x] SVG and PNG output support
  - [ ] JPEG/WebP support
  - [ ] Query parameter support (`?format=`, `?tz=`)
- [x] Timezone Support
  - [x] Timezone abbreviation parsing
  - [x] UTC and offset handling
- [x] Error Handling
  - [x] Comprehensive error responses
  - [x] Parse, render, and rasterization error handling
- [ ] Dynamic Favicon
  - [ ] .ico Rendering
  - [ ] IP to Timezone

## Routes

`GET /[relative|absolute]/[value]{.ext}?tz={timezone}&format={format_string}&now={timestamp}&static={boolean}`

- `{relative|absolute}` - The display format of the time.
- `{value}` - The time value to work with. Relative values can be specified by prefixing with `+` or `-`.
  - Relative values are relative to the value acquired from the `now` parameter (which defaults to the current time).
  - Whether the value is relative or absolute has nothing to do with the display format.
  - `/absolute/+0` will display the current time in UTC.
  - Note: The `now` parameter is returned in the `Date` response header.
- `Accept` or `{.ext}` - Determines the output format. If not specified, `.svg` is assumed.
  - `Accept` requires a valid MIME type.
  - `.ext` requires a valid extension.
  - Supported values: `.png`/`image/png`,
- `X-Timezone` or `?tz={timezone}` - The timezone to display the time in. If not specified, UTC is assumed.
- `?format={format}` - The format of the time to display. If not specified, `%Y-%m-%d %H:%M:%S` is assumed.
  - Only relevant for `absolute` values.
- `X-Date-Now` or `?now={timestamp}` - The timestamp to use for relative time calculations. If not specified, the current time is used.
- `?static={boolean}` - Whether to redirect to a static version of the URL. Useful for creating specific URLs manually.
  - If a value is not passed, (`?static`), `true` is assumed. Anything other than `true`, `1` or `yes` (case-insensitive) is considered `false`.
  - Some header values will be translated to query parameters if provided (not `Accept`).
  - e.g. `/rel/+3600.png?static&now=1752170474` will redirect to `/relative/1752174074.png`

### Examples

```
/1752170474 => 2025-07-10 12:01:14 UTC
/abs/1752170474 => 2025-07-10 12:01:14 UTC
/absolute/+3600 => 2025-07-10 13:01:14 UTC
/abs/-1800 => 2025-07-10 11:01:14 UTC
/rel/1752170474 => 15 minutes ago
/rel/+3600 => 1 hour from now
/relative/-1800 => 30 minutes ago
/relative/1752170474.png?tz=America/Chicago => 2025-07-10 06:01:14 CDT
/relative/1752170474?type=relative => 2081-01-17 12:02:28 PM
```

## Ideas

- Support for different timezone formats in query parameters or headers
  - `?tz=...` or `X-Timezone: ...`
  - `CST` or `America/Chicago` or `UTC-6` or `GMT-6` or `-0600`
  - Automatically guessed based on geolocation of source IP address
- Complex caching abilities
  - Multi-level caching (disk, memory)
  - Automatic expiry of relative items
- `Accept` header support
- IP-based rate limiting
  - Multi-domain rate limiting factors
    - Computational cost: 1ms = 1 token, max 100 tokens per minute
    - Base rate: 3 requests per second
- Cached Conversions
  - If PNG is cached, then JPEG/WEBP/etc. can be converted from cached PNG

### Query Parameters

- `format` - Specify the format of the time string
- `tz` - Specify the timezone of the time string. May be ignored if the time string contains a timezone/offset.

## Structure

1. Routing
   - Handle different input formats at the route layer
2. Parsing
   - Module for parsing input
3. Cache Layer
   - Given all route options, provide a globally available cache for the next layer
4. SVG Template Rendering
   - Template rendering based on parsed input
5. (Optional) Rasterization
   - If rasterization is requested, render SVG to PNG
6. (Catch-all) Error Handling
   - All errors/panics will be caught in separate middleware

## Input Parsing

- Date formatting will be guesswork, but can be specified with `?format=` parameter.
  - To avoid abuse, it will be limited to a subset of the `chrono` formatting options.
- The assumed extension when not specified is `.svg` for performance sake.
  - `.png` is also available. `.jpeg` and `.webp` are planned.
- Time is not required, but will default each value to 0 (except HOUR, which is the minimum specified value).
- Millisecond precision is allowed, but will be ignored in most outputs. Periods or commas are allowed as separators.
- Timezones can be qualified in a number of ways, but will default to UTC if not specified.
  - Fully qualified TZ identifiers like "America/Chicago" are specified using the `tz` query parameter.
  - Abbreviated TZ identifiers like "CST" are specified inside the time string, after the time, separated by a dash.
    - Abbreviated terms are incredibly ambiguous, and should be avoided if possible. For ease of use, they are
      available, but several of them are ambiguous, and the preferred TZ has been specified in code.
    - Full table available in [`abbr_tz`](./src/abbr_tz). Comments designated with `#`. Preferred interpretation
      designated arbitrarily by me. Table sourced
      from [Wikipedia](https://en.wikipedia.org/wiki/List_of_time_zone_abbreviations)
