# time-banner

Dynamically generated timestamp images

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
  - `auto` will attempt to determine the timezone from the client's IP address. Depending on currently unknown factors, this may be disregarded.
- `?format={format}` - The format of the time to display. If not specified, `%Y-%m-%d %H:%M:%S %Z` is assumed.
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

- Frontend with React for Demo
  - Refetch favicon every 10 minutes
  - Click to copy image URLs
  - Dynamic Examples
- Dynamic light/dark mode
  - `?theme={auto|light|dark}`, default `light`
  - Customizable SVG templates
- Dynamic favicon generation
  - Clock svg optimized for favicon size
  - Move hands to the current time
  - Use geolocation of request IP to determine timezone
  - Advanced: create sun/moon SVG based on local time
- Support for different timezone formats in query parameters or headers
  - `?tz=...` or `X-Timezone: ...`
  - `CST` or `America/Chicago` or `UTC-6` or `GMT-6` or `-0600`
  - Automatically guessed based on geolocation of source IP address
- Complex caching abilities
  - Multi-level caching (disk with max size, memory)
  - Automatic expiry of relative items
  - Use browser cache headers
  - Detect force refreshs and allow cache busting
- `Accept` header support
- IP-based rate limiting
  - Multi-domain rate limiting factors
    - Computational cost: 1ms = 1 token, max 100 tokens per minute
    - Base rate: 3 requests per second
- Cached Conversions
  - If PNG is cached, then JPEG/WEBP/etc. can be converted from cached PNG
- Additional date input formats
  - 2025-07-10-12:01:14
  - 2025-07-10-12:01
  - 2025-07-10-12:01:14-06:00
  - 2025-07-10-12:01:14-06
  - 2025-07-10-12:01:14-06:00:00
  - 2025-07-10-12:01:14-06:00:00.000
  - 2025-07-10-12:01:14-06:00:00Z-06:00
