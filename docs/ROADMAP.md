# Roadmap

Sequenced by dependency: each phase unlocks the next. Behavior is defined in
[SPEC.md](./SPEC.md); this file only tracks what exists and what order the rest
arrives in.

- [x] done
- [ ] planned

## Phase 0: shipped

- [x] `/absolute`, `/relative`, and their aliases, plus bare `/{value}`
- [x] Epoch and signed-second values
- [x] SVG and PNG output, extension-selected
- [x] Animated-hands clock favicon as `.ico` and `.png`
- [x] Server-rendered index page with live examples
- [x] Timezone abbreviation table generated at build time (built, not yet
      consumed)
- [x] Health endpoint, structured tracing, compression, request timeout

## Phase 1: foundations

Nothing else can be built cleanly on the current structure, so this comes first.
It adds no user-visible features.

- [x] Reorganize into a workspace: `core`, `render`, `server` (`xtask` deferred
      until Phase 4 gives it something to generate)
- [x] Migrate to a time library with a bundled tz database, ISO 8601 spans, and
      rounding
- [x] Inject the clock rather than reading it inside the renderer
- [x] Typed error enum with stable codes and a single status mapping
- [x] Move rasterization and encoding off the async executor
- [x] Build the font database once, not per request
- [x] Compile templates in; drop the dev-versus-production path fork
- [x] Graceful shutdown with a bounded drain
- [x] Test scaffolding: parameterized cases, property tests, benchmark harness

## Phase 2: value grammar

The parser everything else consumes. Property-testable in isolation once `core`
exists.

- [ ] ISO 8601 instants, durations, and intervals
- [ ] Shorthand dates, `@`-separated date-times, compact dates
- [ ] `now` literal
- [ ] Human durations (`+1y2d3h`), unifying the existing duration parser
- [ ] Digit-count disambiguation between compact dates and epochs
- [ ] `?now=` and the `Date-Now` header as the reference instant
- [ ] Round-trip and never-panic property tests over the whole grammar

## Phase 3: timezones and localization

- [ ] `?tz=` and the `Timezone` header
- [ ] IANA identifiers, abbreviations, fixed and prefixed offsets
- [ ] `~` substitution for `/` in path position
- [ ] Documented abbreviation disambiguation, encoded in the generated table
- [ ] `Timezone` response header on every response
- [ ] `?format=` for absolute output, with bounded expansion
- [ ] `Accept-Language` negotiation and `?locale=`
- [ ] `Content-Language` and `Vary: Accept-Language`
- [ ] Bundled geolocation database for `tz=auto`, with `private` caching
- [ ] Favicon consumes resolved timezone rather than always UTC

## Phase 4: fonts

Blocks styling, because text measurement is wrong until faces are real.

- [ ] `xtask` subsetting pipeline over HarfBuzz, per script and weight
- [ ] Bundle manifest with coverage and license per face
- [ ] Memory-mapped bundle loading; stop loading system fonts
- [ ] Replace estimated text advance with real shaped measurement
- [ ] Ordered fallback chain with substitution reported in a header
- [ ] `?font=` over the manifest
- [ ] Remove the proprietary face; substitute a metric-compatible open one
- [ ] CI regenerates the bundle and verifies it is unchanged

## Phase 5: modes

- [ ] `/countdown` and `/elapsed`
- [ ] `/progress` over intervals, both ISO forms, clamped
- [ ] `/uptime` from process start
- [ ] `/timer` over cron expressions, via an existing parser
- [ ] `/timer` over RRULEs, via an existing RFC 5545 implementation
- [ ] Recurrence evaluated in the resolved timezone
- [ ] Automatic unit selection from distance to target

## Phase 6: styles and themes

- [ ] `plain`, `badge`, `flat`, `segment`, `analog`, `bar`, `tile`
- [ ] Mode and style compatibility matrix, `422` on incompatible pairs
- [ ] `?label=` for badge and bar
- [ ] `?scale=`, bounded
- [ ] `theme=light` and `theme=dark`
- [ ] `theme=auto` via a self-adapting media query in SVG output
- [ ] `theme=auto` via client hints for raster output, with `Accept-CH` and
      `Vary`
- [ ] Retire `/clock` in favor of `/absolute/now?style=analog`

## Phase 7: formats

- [ ] `Accept` negotiation with q-values, exclusions, and server preference
- [ ] Extension takes precedence; unknown extension is `404`
- [ ] `406` with `Link rel=alternate`
- [ ] `Vary: Accept` on every negotiated response, `304` and `406` included
- [ ] WebP, AVIF, JPEG encoders
- [ ] `.txt` and `.json` representations
- [ ] Negotiated error rendering: image, `problem+json`, or plain text

## Phase 8: animation

- [ ] SMIL-animated SVG for countdown, elapsed, timer, and progress
- [ ] Animated GIF with a style-derived palette
- [ ] Animated WebP and APNG
- [ ] Frame budget with frame-rate degradation instead of rejection
- [ ] Terminal clamping at zero
- [ ] Unit coarsening as drift mitigation
- [ ] `Sec-CH-Prefers-Reduced-Motion` and `Save-Data` suppress animation
- [ ] `no-store` on animated responses

## Phase 9: caching and protocol

- [ ] Display-value quantization per mode, with a next-change instant
- [ ] `ETag` over the display value and render inputs
- [ ] `max-age` from the next-change instant
- [ ] `If-None-Match` and `If-Modified-Since`, with correct `304` headers
- [ ] `immutable` for past absolute instants
- [ ] `stale-while-revalidate` where staleness is cosmetic
- [ ] In-memory cache keyed on normalized inputs, weighted by render cost
- [ ] Coalesced concurrent misses
- [ ] Encoded variants derived from a cached pixmap
- [ ] `?static` snapshot redirects, folding headers into the target
- [ ] `Server-Timing` per phase
- [ ] Request correlation with status-proportional log levels

## Phase 10: hardening

- [ ] Dimension, text, format-expansion, interval, and frame bounds
- [ ] Render deadline with `504`
- [ ] Request-rate limit
- [ ] Compute-cost budget calibrated from benchmarks
- [ ] `RateLimit` fields and `Retry-After`
- [ ] Malicious-path classification ahead of budget consumption
- [ ] Dependency auditing across licenses, advisories, and sources
- [ ] Automated dependency updates
- [ ] Container build with a dependency-cached layer

## Phase 11: playground

- [ ] Builder controls for every axis, working without scripting
- [ ] Live preview and copyable URL, updated without a round trip when scripting
      is on
- [ ] Mode and style matrix gallery
- [ ] Format comparison panel with byte sizes
- [ ] Cache-header inspection for the current selection
