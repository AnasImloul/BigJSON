# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2026-06-02

First public release.

### Added
- `jsq` — a streaming, single-pass CLI for querying very large (1+ GB) JSON files
  with a SQL-shaped query language. Emits NDJSON so it composes with `jq`, `head`,
  `wc`, etc.
- Query language: `from`/`join`/`unnest`/`where`/`let`/`distinct`/`aggregate`/
  `collect by`/`having`/`select`/`order by`/`limit`, path grammar (`[]`, `[N]`,
  `["key"]`, `.**`, field-sets), function-call reducers (`count`, `sum`, `avg`,
  `min`, `max`), item-level `where`, `??` defaults, `if()`, scalar functions, and
  correlated subqueries. See [docs/QUERY.md](docs/QUERY.md).
- CLI flags: `--limit`, `--param`, `--stats`, `--stats-only`, `--explain`,
  `--format-only`, plus stdin via `-`.
- BigJSON.app — a native macOS UI over the same engine (streaming open, virtual
  rows, filter-as-you-type, exports).
- Distribution: prebuilt CLI binaries for macOS and Linux (arm64 + x86_64) via a
  Homebrew tap, and a `.dmg` for the macOS app.

[Unreleased]: https://github.com/AnasImloul/jsq/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/AnasImloul/jsq/releases/tag/v1.0.0
