# time-banner specification

How the service behaves: routes, values, styles, formats, headers, caching, and
the parts of the rendering pipeline whose behavior is observable from outside.

Status markers are in [ROADMAP.md](./ROADMAP.md). This document describes the
target behavior, not the current implementation.

## 1. Model

Every response is a pure function of the request and the wall clock. There is no
stored state, no accounts, and nothing to persist. `uptime` is the sole
exception, and it reads only this process's start time.

Three axes are independent and are carried separately.

| Axis   | Carried by            | Meaning                      |
| ------ | --------------------- | ---------------------------- |
| Mode   | first path segment    | which time value is computed |
| Style  | `?style=`             | how that value is drawn      |
| Format | extension or `Accept` | how the drawing is encoded   |

Keeping these separate is deliberate. A countdown drawn as a badge and a
countdown drawn as segment digits are the same value with different
presentation, so they share a mode. An analog clock is not a mode at all, it is
`?style=analog` applied to `/absolute/now`.

```
/countdown/2027-01-01T00:00:00Z.gif?style=badge&theme=dark&font=inter
 └── mode ─┘└──────── value ───────┘└fmt┘└──────────── presentation ───────────┘
```

## 2. Modes

| Mode        | Aliases                | Value      | Renders                         |
| ----------- | ---------------------- | ---------- | ------------------------------- |
| `absolute`  | `abs`, bare `/{value}` | instant    | `2027-01-01 00:00:00 UTC`       |
| `relative`  | `rel`                  | instant    | `2 hours ago`, `in 3 days`      |
| `countdown` | `cd`                   | instant    | `04:12:44`, counting down       |
| `elapsed`   | `since`                | instant    | `312 days`, counting up         |
| `progress`  |                        | interval   | `2026 ▓▓▓▓▓░░░ 62%`             |
| `timer`     |                        | recurrence | time until the next occurrence  |
| `uptime`    |                        | none       | time since this process started |

### 2.1 absolute

Formats the instant in the resolved timezone using `?format=`. Past instants are
immutable and cache accordingly (section 9).

### 2.2 relative

Renders the signed distance between the value and `now`, in words. Language
comes from `Accept-Language` (section 7). The unit is chosen automatically: the
largest unit that leaves the value above 1.

### 2.3 countdown and elapsed

The same computation with opposite signs. `countdown` counts toward a future
instant, `elapsed` counts away from a past one. Either may be rendered
statically or animated (section 8).

A countdown that reaches zero renders its terminal state and stops. The terminal
string is configurable per style; the default is `00:00:00`.

### 2.4 progress

Takes an interval and renders the fraction elapsed. Accepts both ISO interval
forms:

```
/progress/2026-01-01/2027-01-01     explicit start and end
/progress/2026-01-01/P1Y            start and duration
```

Clamps to `[0, 1]`. Before the start it renders 0%, after the end it renders
100%.

### 2.5 timer

Renders the distance to the next occurrence of a recurring event. Two grammars
are accepted, distinguished by prefix:

```
/timer/cron:0 9 * * MON             next Monday 09:00
/timer/rrule:FREQ=MONTHLY;BYDAY=1FR next first-Friday
```

Recurrence parsing is delegated. Cron expressions go through an established cron
parser, RRULEs through an RFC 5545 implementation. Neither grammar is
reimplemented here, and the spec inherits their semantics, including their DST
handling.

Recurrence is evaluated in the resolved timezone, not UTC. `0 9 * * MON` in
`America/Chicago` means 09:00 local, which is a different instant in June than
in January.

### 2.6 uptime

Time since the current process started. Deliberately self-referential and useful
as a deployment badge. It takes no value.

```
/uptime?style=badge     →  up 6 days
```

Because it depends on process identity, it is never cacheable across restarts
and is served `no-store`.

## 3. Values

The grammar has an ISO 8601 spine, because ISO already defines instants,
durations, and intervals, and adopting it wholesale means `countdown`,
`progress`, and `elapsed` share one parser rather than three. Shorthand is
layered on top, because ISO is unpleasant to type by hand.

| Form                 | Example                 | Notes                             |
| -------------------- | ----------------------- | --------------------------------- |
| Epoch seconds        | `1752170474`            | integer, no sign                  |
| ISO instant          | `2027-01-01T00:00:00Z`  | offsets accepted                  |
| ISO date             | `2027-01-01`            | midnight in the resolved timezone |
| Compact date         | `20270101`              |                                   |
| Date and time        | `2027-01-01@14:30`      | `@` avoids escaping a space       |
| Literal              | `now`                   | current instant                   |
| Signed seconds       | `+3600`, `-1800`        | relative to `now`                 |
| Human duration       | `+1y2d3h`, `-30m`       | relative to `now`                 |
| ISO duration         | `+P1Y2D`, `-PT30M`      | relative to `now`                 |
| ISO interval         | `2026-01-01/2027-01-01` | `progress` only                   |
| Interval by duration | `2026-01-01/P1Y`        | `progress` only                   |

Signed forms are relative to the reference instant, which is `now` unless
overridden by `?now=` or the `Date-Now` request header. Whether a value is
written relatively has nothing to do with which mode renders it: `/absolute/+0`
renders the current time as an absolute string.

Unparseable values are `400`, rendered per section 11.

### 3.1 Ambiguity

`20270101` could be a date or an epoch. Digit-count decides: exactly 8 digits
parses as a compact date, anything else as epoch seconds. A leading `+` or `-`
always means relative. This is stated because the shorthand exists to be typed,
and a typist needs to know which reading wins.

## 4. Styles

`?style=` selects presentation. Default is `plain`.

| Style     | Description                             |
| --------- | --------------------------------------- |
| `plain`   | text on a transparent ground, no chrome |
| `badge`   | shields-style label and value, two-tone |
| `flat`    | single rounded pill, one fill           |
| `segment` | seven-segment digits                    |
| `analog`  | clock face with hands                   |
| `bar`     | horizontal progress bar with a label    |
| `tile`    | calendar-style date block               |

Not every pairing is meaningful. Requests for an incompatible pairing are `422`.

|             | plain | badge | flat | segment | analog | bar | tile |
| ----------- | ----- | ----- | ---- | ------- | ------ | --- | ---- |
| `absolute`  | yes   | yes   | yes  | yes     | yes    | no  | yes  |
| `relative`  | yes   | yes   | yes  | no      | no     | no  | no   |
| `countdown` | yes   | yes   | yes  | yes     | no     | yes | no   |
| `elapsed`   | yes   | yes   | yes  | yes     | no     | no  | no   |
| `progress`  | yes   | yes   | yes  | no      | no     | yes | no   |
| `timer`     | yes   | yes   | yes  | yes     | no     | yes | no   |
| `uptime`    | yes   | yes   | yes  | yes     | no     | no  | no   |

`analog` requires a wall-clock time, so it pairs only with `absolute`. `bar`
requires a bounded fraction, so it needs either an interval or a countdown with
a known origin.

## 5. Presentation parameters

| Parameter | Default                | Values                                       |
| --------- | ---------------------- | -------------------------------------------- |
| `style`   | `plain`                | section 4                                    |
| `theme`   | `auto`                 | `auto`, `light`, `dark`                      |
| `font`    | `inter`                | any family in the bundle manifest            |
| `scale`   | `1`                    | `0.5` to `4`, rendered dimensions multiplier |
| `label`   | none                   | badge label text, `badge` and `bar` only     |
| `format`  | see below              | `strftime`-style, `absolute` only            |
| `locale`  | from `Accept-Language` | BCP 47 tag                                   |
| `now`     | current instant        | reference instant for relative values        |
| `tz`      | `UTC`                  | section 6                                    |
| `static`  | `false`                | section 9.4                                  |

`format` defaults to `%Y-%m-%d %H:%M:%S %Z`. Expansion is bounded (section 12).

### 5.1 Themes

`auto` resolves differently per format, and the asymmetry is worth stating
because it is the main reason to prefer SVG.

An SVG carries its own `prefers-color-scheme` media query. The server emits one
document containing both palettes and the client picks, so `auto` works in a
GitHub README with no negotiation, no client hints, and no cache variation at
all.

Raster formats cannot do this. A PNG is one palette. For those, `auto` reads
`Sec-CH-Prefers-Color-Scheme` and falls back to `light` when the hint is absent.
The service advertises the hint via `Accept-CH`. Responses that consumed it
carry `Vary: Sec-CH-Prefers-Color-Scheme`.

Explicit `theme=light` or `theme=dark` bypasses all of this and produces a
single palette in every format.

### 5.2 Reduced motion

`Sec-CH-Prefers-Reduced-Motion: reduce` suppresses animation. Animated formats
serve a single representative frame instead, and animated SVG omits its SMIL
blocks. This is the accessibility-correct behavior and it costs nothing at
render time.

`Save-Data: on` has the same effect, plus a downgrade to the cheapest acceptable
format.

## 6. Timezones

Resolution order, first match wins:

1. `?tz=`
2. `Timezone` request header
3. `?tz=auto` or `Timezone: auto`, resolved by IP geolocation
4. `UTC`

Accepted forms:

| Form            | Example           |
| --------------- | ----------------- |
| IANA identifier | `America/Chicago` |
| Abbreviation    | `CST`, `CDT`      |
| Fixed offset    | `-0600`, `-06:00` |
| Prefixed offset | `UTC-6`, `GMT-6`  |

Path segments containing `/` are a problem for IANA identifiers in path
position. In query position they are fine. Where an identifier must appear in a
path, `~` substitutes for `/`: `America~Chicago`.

The resolved zone is echoed in the `Timezone` response header, always,
including when it was resolved by geolocation or defaulted. A zone that has an
IANA identifier is reported by it; a fixed offset has none and is reported as
`±HH:MM`.

Offsets are read literally, so `UTC-6` is six hours behind UTC. This is the
ISO reading, and the opposite of the POSIX `TZ` convention, where the same
string means six hours ahead.

### 6.1 Abbreviation ambiguity

Abbreviations are not unique. `CST` is US Central, China Standard, and Cuba
Standard. `IST` is India, Ireland, and Israel. `BST` is British Summer and
Bangladesh Standard. The abbreviation table has to encode a choice, so the
choice is stated rather than left implicit:

- Ambiguous abbreviations resolve to whichever reading dominates in
  English-language usage, which is not the same as whichever covers the most
  people. `CST` is US Central, though China Standard Time covers fifteen times
  as many. `BST` is British Summer, though Bangladesh Standard covers more.
  `IST` is India, which happens to be both.
- An abbreviation resolves to a region, and therefore to that region's IANA
  zone, not to the offset the name literally states. `?tz=CST` in July renders
  CDT. A caller who wants a fixed offset should ask for one: `?tz=-06:00`.
- Each abbreviation resolves to exactly one zone, recorded in the generated
  table rather than decided at request time.
- The `Timezone` response header carries the resolved IANA identifier, never
  the abbreviation, so a caller can see which reading they got.
- Callers who need a specific reading should send an IANA identifier.

Abbreviations are matched ahead of IANA identifiers, because tzdb carries
legacy `EST`, `MST`, and `HST` zones that never observe daylight saving.
Matching those first would make `?tz=EST` an hour wrong all summer.

### 6.2 Geolocation

`auto` resolves the client address against a bundled geolocation database, which
maps to a country or region and then to an IANA zone. Accuracy is coarse and
countries spanning multiple zones resolve to their most populous one.

Geolocation is best-effort. A miss falls through to `UTC` rather than erroring.

Responses that consumed the client address carry `Cache-Control: private` and
are never shared between clients.

## 7. Localization

`relative`, `countdown`, `elapsed`, and `timer` render words, and those words
are localized. `?locale=` overrides, otherwise the language is negotiated from
`Accept-Language`.

Unsupported languages fall back to the closest supported tag by BCP 47 matching,
then to `en`. Responses carry `Content-Language` and `Vary: Accept-Language`.

Localization interacts with fonts. A locale whose script the selected face does
not cover triggers the fallback chain in section 15.3.

## 8. Formats

| Extension | Media type         | Animated | Notes                             |
| --------- | ------------------ | -------- | --------------------------------- |
| `.svg`    | `image/svg+xml`    | via SMIL | default, theme-adaptive           |
| `.png`    | `image/png`        | no       |                                   |
| `.gif`    | `image/gif`        | yes      | 256-color palette                 |
| `.webp`   | `image/webp`       | yes      |                                   |
| `.avif`   | `image/avif`       | yes      | best compression, slowest encode  |
| `.apng`   | `image/apng`       | yes      |                                   |
| `.jpg`    | `image/jpeg`       | no       | no transparency                   |
| `.ico`    | `image/x-icon`     | no       | favicon only                      |
| `.txt`    | `text/plain`       | no       | the rendered string, nothing else |
| `.json`   | `application/json` | no       | parsed request and computed value |

`.txt` and `.json` are not decoration. They make the service scriptable and they
make the parser observable, which is what `?debug` would otherwise be for.

### 8.1 Selection

An extension in the path selects the format outright and `Accept` is not
consulted. An unrecognized extension is `404`, because the path names a resource
that does not exist.

Without an extension, the format is negotiated from `Accept` by q-value, with
ties broken by specificity and then by server preference (`AVIF`, `WebP`, `PNG`,
`SVG`). A `q=0` entry excludes that range explicitly. A request with no `Accept`
at all is treated as `*/*` and served SVG.

If nothing acceptable can be produced, the response is `406` with a `Link`
header listing alternates.

Every negotiated response carries `Vary: Accept`, including `304` and `406`.

## 9. Caching

The interesting property of a time banner is that its rendered content changes
on a schedule that is knowable in advance. A relative badge reading
`2 hours ago` will not change until it reads `3 hours ago`. That is the basis
for both validators and freshness here, rather than a guessed TTL.

### 9.1 Quantization

Each mode computes a **display value**: the exact string or geometry that will
be drawn, after unit selection and rounding. Two requests with different
instants but the same display value are the same representation.

From the display value, two things follow:

- The `ETag` is computed over the display value plus every input that affects
  rendering (style, theme, font, scale, locale, format). It does not include the
  request instant.
- `max-age` is the number of seconds until the display value would next change.

A `/relative/` badge one hour into "2 hours ago" therefore revalidates with a
`304` and transfers no bytes, and its cache expires at the moment it becomes
wrong rather than at an arbitrary interval.

### 9.2 Per-mode freshness

| Mode                               | Cache-Control                                            |
| ---------------------------------- | -------------------------------------------------------- |
| `absolute`, past instant           | `public, max-age=31536000, immutable`                    |
| `absolute`, future instant         | `public, max-age=` until it becomes past                 |
| `relative`, `elapsed`, `countdown` | `public, max-age=` until the display value changes       |
| `progress`                         | `public, max-age=` until the rendered percentage changes |
| `timer`                            | `public, max-age=` until the next occurrence changes     |
| `uptime`                           | `no-store`                                               |
| any, animated                      | section 10                                               |
| any, geolocated                    | `private` prefix                                         |

Validators are strong when the response bytes are deterministic for the ETag
inputs, and weak (`W/`) when they are not, which is the case for formats whose
encoders are not byte-reproducible.

`If-None-Match` and `If-Modified-Since` are both honored. `304` responses carry
the same `Vary`, `Cache-Control`, and `ETag` headers as the `200` would have.

`stale-while-revalidate` is emitted for modes whose staleness is cosmetic rather
than wrong, which is every mode except `countdown` near zero.

### 9.3 Proxy interaction

GitHub serves README images through a caching image proxy. It respects neither
short `max-age` nor `no-store` reliably, and it will hold a response for hours.
This is a real constraint, not a hypothetical one, and it shapes what is honest
to promise:

- Static banners are unaffected. A `2 hours ago` badge held for four hours reads
  `2 hours ago` and is wrong by exactly the cache age.
- Coarse units hide this. At day granularity a multi-hour cache is invisible,
  which is why automatic unit selection matters more than it looks.
- Animated output cannot hide it. See section 10.

### 9.4 Snapshots

`?static` redirects to a URL whose rendering is fixed. Relative values are
resolved against the reference instant and rewritten as absolute, so the target
is immutable and cacheable forever.

```
/rel/+3600.png?static&now=1752170474  →  302  /rel/1752174074.png
```

Header-supplied values are folded into the target as query parameters, except
`Accept`, which cannot be represented in a URL and is instead resolved to a
concrete extension. Absent a value, `?static` means `?static=true`. Anything
other than `true`, `1`, or `yes`, case-insensitively, is false.

## 10. Animation

Both animated paths are first-class, and they reach different surfaces.

**Animated SVG** uses SMIL. GitHub's image proxy passes SVG through and the
sanitizer strips scripts and external references but not SMIL, so animation
survives in a README. It costs no frames, no encoder, and almost no bytes.

**Animated GIF** reaches HTML email, where SVG does not render at all. It costs
a per-frame rasterization and a palette quantization pass, making it the most
expensive thing the service produces by a wide margin.

### 10.1 Drift

An animated artifact has no clock. Once rendered, it cannot know what time it
is; it can only play a timeline that was baked at render time. A countdown baked
at 10:00 and served from a cache at 14:00 animates smoothly and is four hours
wrong.

Nothing fixes this without a clock, and there is no clock available inside a
sanitized SVG or a GIF. The spec says so rather than pretending otherwise.

Default behavior: **short loop, aggressively uncacheable.** Roughly 60 seconds
of frames, `Cache-Control: no-store`, correct wherever cache directives are
honored, which is direct browser loads and most email clients. Behind a proxy
that ignores them, it is wrong by the cache age and it says so here.

Two mitigations apply automatically:

- Unit coarsening. Granularity is selected from distance to the target: seconds
  under an hour, minutes under a day, days beyond. Past a day, cache-induced
  drift is smaller than one displayed unit and is therefore invisible.
- Terminal clamping. A countdown that would pass zero mid-loop clamps at zero
  rather than wrapping into negative time.

`?static` is the escape hatch for a caller who wants a guaranteed-correct
artifact.

### 10.2 Frame budget

Animated output is bounded by frame count, not duration, so the cost is
predictable regardless of the requested loop length. Requests exceeding the
budget get a lower frame rate rather than an error. GIF palettes are generated
per render from the style's actual colors, which for a two-tone badge is a
handful of entries rather than an adaptive 256-color pass.

## 11. Errors

The caller is usually an `<img>` tag. It never sees a response body, so a JSON
error is invisible and the reader gets a broken-image icon carrying no
information.

Errors are therefore negotiated like anything else. A caller that accepts images
gets a rendered error banner stating the problem, in the requested format, at
the requested status code. A caller that accepts JSON gets
`application/problem+json`. A caller that accepts neither gets `text/plain`.

| Status | Cause                                                         |
| ------ | ------------------------------------------------------------- |
| `400`  | value or parameter could not be parsed                        |
| `404`  | unknown route or unknown extension                            |
| `406`  | no acceptable format                                          |
| `413`  | request would exceed a render bound (section 12)              |
| `415`  | recognized format that cannot express the request             |
| `422`  | parsed but incoherent, such as an incompatible mode and style |
| `429`  | rate limited                                                  |
| `500`  | render failure                                                |
| `504`  | render exceeded its deadline                                  |

Error banners are `no-store`. A transient failure must not be cached into a
README for hours.

Every error carries a stable machine-readable code alongside its human message,
so callers can branch without matching on prose.

## 12. Bounds

A stateless renderer's abuse surface is compute, not data. Every unbounded input
is a bound.

| Input                     | Bound                                            |
| ------------------------- | ------------------------------------------------ |
| Rendered width and height | capped in pixels, independently                  |
| `scale`                   | `0.5` to `4`                                     |
| `format` expansion        | capped output length, evaluated during expansion |
| Label and text length     | capped in characters before layout               |
| Animation frames          | capped per response                              |
| Interval span             | capped, to bound progress-bar geometry           |
| Render wall time          | deadline, `504` past it                          |

`format` deserves specific mention: a short `strftime` string can expand to
arbitrary length, so expansion is bounded during evaluation rather than after.

### 12.1 Rate limiting

Two dimensions, because request count alone does not describe the load. Encoding
an animated AVIF and returning a cached SVG differ by orders of magnitude.

- A request-rate limit, applied to everyone.
- A compute-cost budget, where a request's cost is its measured render time and
  the budget refills over time. Cache hits cost approximately nothing and are
  effectively unlimited.

The cost table is derived from benchmarks (section 17), not estimated.

Limits are reported with the standard `RateLimit` fields and `Retry-After` on
`429`. Malicious-looking paths are classified before any budget is consumed, so
scanner traffic does not deplete a legitimate caller's allowance.

## 13. Headers

Request headers, all optional.

| Header                          | Effect                                   |
| ------------------------------- | ---------------------------------------- |
| `Accept`                        | format negotiation                       |
| `Accept-Language`               | localization                             |
| `Timezone`                      | timezone, same grammar as `?tz=`         |
| `Date-Now`                      | reference instant for relative values    |
| `If-None-Match`                 | conditional request                      |
| `If-Modified-Since`             | conditional request                      |
| `Sec-CH-Prefers-Color-Scheme`   | resolves `theme=auto` for raster formats |
| `Sec-CH-Prefers-Reduced-Motion` | suppresses animation                     |
| `Save-Data`                     | suppresses animation, downgrades format  |

Query parameters take precedence over headers, since a URL is the only thing a
README author can control.

`Timezone` and `Date-Now` are unprefixed deliberately. The `X-` convention was
deprecated by [RFC 6648](https://www.rfc-editor.org/rfc/rfc6648) and there is no
reason to carry it.

Response headers.

| Header             | Meaning                                            |
| ------------------ | -------------------------------------------------- |
| `Content-Type`     | negotiated format                                  |
| `Content-Language` | resolved locale                                    |
| `Cache-Control`    | section 9                                          |
| `ETag`             | section 9.1                                        |
| `Last-Modified`    | display value's start instant                      |
| `Vary`             | every negotiated input that affected this response |
| `Date`             | reference instant used                             |
| `Timezone`         | resolved IANA identifier                           |
| `Accept-CH`        | client hints the service will use                  |
| `Link`             | `rel=alternate` for other formats                  |
| `RateLimit`        | remaining budget                                   |
| `Server-Timing`    | per-phase render timings                           |
| `Retry-After`      | on `429` and `503`                                 |

`Server-Timing` reports `parse`, `render`, `raster`, `encode`, and a `cache`
entry describing hit or miss. It is genuinely useful for a service whose whole
cost is rendering, and it is visible in browser devtools without
instrumentation.

## 14. Pipeline

```
request
  → parse      URL, value grammar, parameters, headers
  → resolve    timezone, locale, theme, font, reference instant
  → compute    mode logic → display value + quantization interval
  → cache      key from resolved inputs; hit returns encoded bytes
  → template   display value + style → SVG document
  → raster     SVG → pixmap                       (raster formats only)
  → encode     pixmap → PNG/GIF/WebP/AVIF/...     (raster formats only)
  → respond    headers from the quantization interval
```

SVG output skips raster and encode entirely, which is why it is the default and
the cheapest path.

Rasterization and encoding are CPU-bound and run off the async executor.
Everything before them is cheap enough to run inline.

### 14.1 Text measurement

Canvas size must fit the text. The current implementation multiplies character
count by a hardcoded advance ratio, which is wrong for any font that is not
monospace and wrong for monospace fonts whose ratio differs.

Real measurement means shaping the string with the actual face and reading the
resulting advance. The shaper is already present as a transitive dependency of
the renderer, so this is a correction rather than a new capability.

### 14.2 Cache

A single in-memory cache, keyed on the fully resolved render inputs after
normalization, so equivalent requests written differently share an entry.
Entries are weighted by render cost, so an expensive animated AVIF is retained
in preference to a cheap SVG under memory pressure.

Concurrent misses on the same key coalesce into one render.

Encoded variants for a given pixmap are derived from the rasterized result
rather than re-rendering from SVG, so a cached PNG makes its JPEG and WebP
siblings nearly free.

## 15. Fonts

Fonts are complex and the complexity is load-bearing. Nothing here reimplements
any part of font handling; the pipeline delegates to established tools and the
service delegates to the renderer's shaper.

### 15.1 Bundle

Faces are subsetted per script by HarfBuzz's subsetter, invoked from an `xtask`.
Each family produces one artifact per (weight, script) pair, alongside a
manifest describing coverage.

Generation is an explicit build step, not a `build.rs` step. Subsetting a font
library is too slow and too native-toolchain-dependent to sit in the compile
graph of a service that gets type-checked constantly.

The resulting bundle is not committed. `just assets` produces it, CI produces
it, and CI verifies that regenerating from unchanged sources produces an
unchanged bundle.

### 15.2 Loading

The bundle ships beside the binary and is memory-mapped at startup, so faces
page in lazily and only the faces actually used occupy resident memory. This
scales to a large library without a proportionally large binary or a large
baseline footprint.

System fonts are never loaded. Rendering must be identical across machines, and
scanning system font directories makes it neither identical nor fast.

### 15.3 Fallback

Per-script subsetting guarantees that some request will select a face lacking a
glyph it needs, because `Accept-Language` and `format` can both introduce
characters the requested family does not cover.

Resolution walks an ordered chain: the requested face, then a broad-coverage
face for the resolved script, then a last-resort face, then the missing-glyph
box. Substitutions are reported in a response header so a caller can tell what
happened rather than wondering why their font looks wrong.

### 15.4 Licensing

Every bundled face is openly licensed, with its license recorded in the manifest
and carried into the build output. Faces that cannot be redistributed are not
bundled, and metric-compatible open substitutes are used where a proprietary
face would otherwise be the obvious choice.

## 16. Web interface

A playground at `/`, server-rendered and functional without JavaScript. Controls
for mode, value, style, theme, font, and format produce a live preview and a
copyable URL.

Scripting is an enhancement, not a requirement: without it the page still
renders, still shows examples, and its controls still work through form
submission. With it, the preview and URL update as controls change, without a
round trip.

Alongside the builder, a gallery renders the mode and style matrix so the
compatibility table in section 4 is visible rather than merely asserted, and a
format panel shows the same banner encoded every way with its byte size.

## 17. Architecture

```
crates/
  core/      value grammar, modes, quantization, timezone resolution
  render/    templates, fonts, rasterization, encoders
  server/    routing, negotiation, caching, middleware, playground
xtask/       font bundle, timezone table, verification
assets/      generated, not committed
docs/        this document, roadmap
```

`core` has no I/O and no web framework. That is what makes property testing the
value grammar cheap, and the grammar is the part of this project most worth
testing that way.

Time handling uses a library with a bundled timezone database, native ISO 8601
duration and interval parsing, and rounding primitives. Rounding is not
incidental: quantization (section 9.1) is the basis of the caching design, and
hand-rolling it would be the same mistake as hand-rolling font subsetting.

### 17.1 Conventions

- Errors are typed enums with stable machine-readable codes, mapped once to
  status and code rather than at each call site. No stringly-typed error
  payloads.
- The clock is injected, not read from inside the renderer. This makes rendering
  deterministic under test and makes `?now=` fall out of the design rather than
  being bolted onto it.
- Templates and generated tables are compiled in. There is no
  dev-versus-production path fork for asset loading.
- Tests use parameterized cases for grammar coverage and property tests for
  parser invariants: round-tripping, and never panicking on arbitrary input.
- Benchmarks cover each (mode, style, format) combination and produce the cost
  table that section 12.1 calibrates against.
- Dependency auditing covers licenses, advisories, and sources, not advisories
  alone.

## 18. Deployment

A container carrying the binary, the font bundle, and the geolocation database.
Both data assets are memory-mapped, versioned independently of the application,
and refreshed without rebuilding it.

Targets:

| Property                      | Target                                        |
| ----------------------------- | --------------------------------------------- |
| SVG render, cache miss        | low single-digit milliseconds                 |
| PNG render, cache miss        | tens of milliseconds                          |
| Animated GIF, cache miss      | hundreds of milliseconds                      |
| Any format, cache hit         | sub-millisecond                               |
| Cold start to first response  | under a second                                |
| Resident memory, steady state | bounded by the cache budget plus mapped pages |

The service shuts down gracefully, draining in-flight renders under a bounded
deadline before exiting.

## 19. Open questions

- Whether `progress` should accept an open-ended interval, and what it would
  render.
- Whether the compute-cost budget should be per-address or per-referrer, given
  that a popular README concentrates traffic from many addresses onto one
  banner.
- Whether animated AVIF earns its encode cost over animated WebP at banner
  sizes.
- Whether the geolocation database and the font bundle should share one asset
  format and one refresh mechanism rather than two.
- What `timer` should render when a recurrence has no next occurrence.
