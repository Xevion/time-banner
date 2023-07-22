# time-banner

My first Rust project, intended to offer a simple way to display the current time relative, in an image format.

## Planned Features

- Dynamic light/dark mode
    - Via Query Parameters for Raster or SVG
    - Via CSS for SVG
- Relative or Absolute Format
- Dynamic Formats
- Caching Abilities
    - Relative caching for up to 59 seconds, purged on the minute
    - Absolute caching for up to 50MB, purged on an LRU basis
- Flexible & Dynamic Browser API
    - Allow users to play with format in numerous ways to query API
    - Examples
        - `/svg/2023-06-14-3PM-CST`
        - `2023-06-14-3PM-CST.svg`
        - `/jpeg/2023.06.14.33` (14th of June, 2023, 2:33 PM UTC)
        - `/jpeg/2023.06.14.33T-5` (14th of June, 2023, 2:33 PM UTC-5)

## Routes

```shell
/{time}[.{ext}]
/{rel|relative}/{time}[.{ext}]
/{abs|absolute}/{time}[.{ext}]
```

- If relative or absolute is not specified, it will be the opposite of the time string's format.

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
