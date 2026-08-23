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

- [x] Reorganize into a workspace: `core`, `render`, `server`, `xtask` (the
      last added once font fetching gave it something to do)
- [x] Migrate to a time library with a bundled tz database, ISO 8601 spans, and
      rounding
- [x] Inject the clock rather than reading it inside the renderer
- [x] Typed error enum with stable codes and a single status mapping
      (the per-variant code exists on `core::ParseError` but isn't surfaced
      in the JSON body yet - every parse failure reports the same generic
      "ParseError", not e.g. "unknown_timezone"; tracked as a phase 7 item)
- [x] Move rasterization and encoding off the async executor
- [x] Build the font database once, not per request
- [x] Compile templates in; drop the dev-versus-production path fork
- [x] Graceful shutdown with a bounded drain
- [x] Test scaffolding: parameterized cases, property tests, benchmark harness

## Phase 2: value grammar

The parser everything else consumes. Property-testable in isolation once `core`
exists.

- [x] ISO 8601 instants, durations, and intervals
- [x] Shorthand dates, `@`-separated date-times, compact dates
- [x] `now` literal
- [x] Human durations (`+1y2d3h`), unifying the existing duration parser
- [x] Digit-count disambiguation between compact dates and epochs
- [x] `?now=` and the `Date-Now` header as the reference instant
- [x] Round-trip and never-panic property tests over the whole grammar

## Phase 3: timezones and localization

- [x] `?tz=` and the `Timezone` header
- [x] IANA identifiers, abbreviations, fixed and prefixed offsets
- [x] `~` substitution for `/` in path position
- [x] Documented abbreviation disambiguation, encoded in the generated table
- [x] `Timezone` response header on every response
- [x] `?format=` for absolute output, with bounded expansion
- [x] `Accept-Language` negotiation and `?locale=`
- [x] `Content-Language` and `Vary: Accept-Language`
- [x] Bundled geolocation database for `tz=auto`, with `private` caching
      (DB-IP City Lite, converted to a `geoip.bin` table by `xtask geoip`; a
      miss still falls through to UTC)
- [x] Favicon consumes resolved timezone rather than always UTC
- [x] `/`, `/favicon.ico`, and `/favicon.png` default `tz` to `auto` rather
      than `UTC`, since nothing there is a shared badge

## Phase 4: fonts

Blocks styling, because text measurement is wrong until faces are real.

- [x] `xtask` subsetting pipeline (`skera`, in `xtask/src/fonts.rs`; one static
      artifact per family rather than per weight and script, since dropping the
      variation tables pins the weight and `?format=` makes a per-script split
      pointless). 1.52 MB of upstream faces to 426 KB, with advances checked
      unchanged against the upstream face for every bundled script
- [x] Bundle manifest with coverage and license per face (coverage read back
      from each subsetted `cmap`, copyright and license URL read off the built
      artifact, plus a SHA-256 per face; the manifest is the only committed
      part of the bundle)
- [x] Replace estimated text advance with real shaped measurement (`render/font.rs`
      shapes with `harfrust` over the same bytes `usvg` uses, so SVG and PNG
      agree on canvas size by construction)
- [x] Ordered fallback chain with substitution reported in a header (whole-string,
      not per-cluster; the `Font` response header lists the faces tried in order)
- [x] `?font=` over the bundled families, with a per-mode default
- [x] Decide how SVG output carries its font: `?text=outline|embed|live`
      (section 15.5). Both real strategies are built; `outline` is the default
- [x] Remove the proprietary face; substitute a metric-compatible open one
      (Arial → Arimo, all three bundled faces now OFL-1.1 via Google Fonts,
      commit-pinned and checksum-verified in `xtask/src/fonts.rs`)
- [x] CI regenerates the bundle and verifies it is unchanged (`just fonts-verify`;
      compares the manifest it would write against the committed one, which
      covers the face bytes via the recorded checksums)
- [x] Retain the OFL `name` records through both subsetting passes, so an
      embedded face carries the notice and license the license requires
- [ ] Memory-mapped bundle loading. Deliberately not done: a compiled-in blob
      is already demand-paged from the executable, and unlike `geoip.bin` a
      missing font bundle has no degraded mode (section 15.2). Revisit only if
      the face set grows enough to make binary size matter
- [ ] Revisit `?text=embed` once a subsetter can instance variable fonts;
      `skera` 0.6 cannot pin an axis, which is why optical sizing is off
      (section 15.6) rather than a per-request choice

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
- [ ] Surface `ParseError::code()` (and equivalents) as the JSON error
      body's `error` field instead of the outer `TimeBannerError` variant
      name

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
