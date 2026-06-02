# Contributing

Thanks for your interest in improving jsq. This is a small project, so the process
is light.

## Repo layout

- `engine/` — the Rust crate: query engine, parser, evaluator, FFI, and the `jsq`
  binary. Self-contained; builds on macOS, Linux, and Windows.
- `app/` — the SwiftUI macOS app. A thin layer over the engine's C ABI.
- `scripts/` — shared build helpers (`build-engine.sh`, `release.sh`).

Every semantic decision — query syntax, evaluation, output formatting — lives in
`engine/`. The app consumes results and adds no query logic of its own, so most
contributions land in `engine/`.

## Engine (Rust)

```sh
cd engine
cargo build --all-targets        # library + jsq binary
cargo test --all-targets         # unit + integration tests
cargo fmt --all                  # format before committing
cargo clippy --all-targets       # lint
```

The surface query language lives in `engine/src/query/surface/` (parser, lowerer,
formatter). The grammar vocabulary is defined once in `engine/src/query/grammar.rs`
and verified by `engine/tests/grammar_manifest.rs` — if you add or rename a keyword,
update the grammar table and that manifest test will keep everything in sync.

`engine/tests/query_surface.rs` has runnable examples covering every clause; it's the
best place to add a regression test for a language change.

## macOS app (Swift)

```sh
cd app
open BigJSON.xcodeproj            # then ⌘R, or:
xcodebuild -project BigJSON.xcodeproj -scheme BigJSON -configuration Debug build
```

The "Build Rust engine" build phase runs `scripts/build-engine.sh` automatically, so
`cargo` is the only out-of-band dependency.

## Pull requests

- Keep changes focused; one logical change per PR.
- Run `cargo test`, `cargo fmt`, and `cargo clippy` before opening a PR. CI runs the
  engine tests on Linux and a full app build on macOS.
- Add or update tests for behavior changes, especially anything touching the query
  language.
- Update [docs/QUERY.md](docs/QUERY.md) and [CHANGELOG.md](CHANGELOG.md) when you
  change user-facing syntax or behavior.

## Reporting bugs

Open an issue with the `jsq` version (`jsq --version`), the query, and a minimal
JSON input that reproduces the problem. For a query that parses but behaves
unexpectedly, the output of `jsq --explain <FILE> '<QUERY>'` is useful.
