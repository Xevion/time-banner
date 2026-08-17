# time-banner

Dynamically generated time images. Shields.io, but for time.

A lot of surfaces render images but forbid scripts and cannot show live text:
GitHub READMEs, HTML email, forum signatures, Notion pages, Discord embeds.
`time-banner` renders a time value as an image so those surfaces can display one
anyway.

```markdown
![](https://time-banner.xevion.dev/relative/1752170474)
![](https://time-banner.xevion.dev/absolute/-1800.png)
```

## Shape

Three independent axes.

| Axis   | Carried by            | Answers                   |
| ------ | --------------------- | ------------------------- |
| Mode   | path segment          | which time value          |
| Style  | `?style=`             | how it looks              |
| Format | extension or `Accept` | how it goes over the wire |

```
/countdown/2027-01-01T00:00:00Z.gif?style=badge&theme=dark
 └── mode ─┘└──────── value ───────┘└ fmt ┘└───── style ─────┘
```

Only `absolute` and `relative` are implemented today; the rest of the mode,
style, and value grammar in the docs below is the target, not the current
behavior.

```
/1752170474                    2025-07-10 12:01:14 UTC
/rel/-1800                     30 minutes ago
/rel/+1h30m                    in an hour and a half
/absolute/+3600.png            an hour from now, as a PNG
```

## Documentation

- [docs/SPEC.md](docs/SPEC.md) covers every route, parameter, header, format,
  and the caching and animation semantics.
- [docs/ROADMAP.md](docs/ROADMAP.md) tracks what is built and what is planned.

## Development

```bash
just fonts      # fetch the bundled fonts (once; cached after that)
just            # list recipes
just check      # format-check, lint, test
just test       # cargo nextest run
cargo run       # run the server
```

Requires a recent stable Rust toolchain. The font files aren't committed
(section 15.1); `just check`/`just test`/`just build` fetch them automatically,
but a bare `cargo run` or `cargo build` needs `just fonts` run first.

## License

MIT.
