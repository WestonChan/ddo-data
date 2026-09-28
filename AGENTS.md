# ddo-data

Rust workspace that turns Maetrim's DDOBuilderV2 data files into SQLite and serves it as a read-only HTTP API. This file is the instruction set for every coding agent working here (Claude Code, Codex, and others that read `AGENTS.md`); `CLAUDE.md` is a symlink to it.

## Quick Reference

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"   # cargo is keg-only on the maintainer's Mac
cargo test --workspace
cargo lint                          # clippy -D warnings, then the no-comments check; what CI runs
cargo fmt --all --check
cargo xtask no-comments --fix       # strip every comment and doc comment
cargo run -p ddo-etl -- build --source upstream/Output/DataFiles --out ddo.db
DDO_DB_PATH=ddo.db ICONS_DIR=icons cargo run -p ddo-api
```

## Structure

- `crates/ddo-model` — the schema as Rust: enums, seeds, DDL, `SCHEMA_VERSION`. Based on ddo-tools' schema, not on DDOBuilderV2's XML; the ETL is the adapter.
- `crates/ddo-etl` — parses `Output/DataFiles` with quick-xml and writes SQLite. Mapping vocabularies are data in `data/*.toml`; an unmapped value fails the build on purpose.
- `crates/ddo-api` — axum read API over the ETL output. OpenAPI at `/docs`, strong ETags, `X-Dataset-Version`, rate limiting, `/v1/dump.sqlite`, icons at `/icons/`.
- `xtask` — repo tooling behind `cargo xtask`. Not published.
- `upstream/` — gitignored sparse checkout of DDOBuilderV2; `.github/workflows/deploy.yml` refreshes it weekly and deploys to Fly.io.
- Planning lives in the ddo-tools roadmap (`docs/roadmap.md`, V-series section), not here.

## Code Quality

- **No comments, no doc comments.** Code carries its meaning in names, types, and tests. Do not write `//`, `/* */`, `///`, or `//!`. If something needs a comment to be understood, rename, split, or restructure it until it doesn't. Reasoning that cannot live in code goes in the ddo-tools roadmap or `docs/` there. `cargo lint` fails on any comment; `cargo xtask no-comments --fix` strips them.
- **Specific things get purpose names; shared things get general names.** A function that does one job is named for the job (`bonuses_via`, `read_item_buffs`), not its mechanism. Code in `ddo-model` and shared helpers describes the capability, not the first caller.
- **Keep code clean and refactor freely.** Improve adjacent code you touch; don't leave a file worse than you found it.
- `rustfmt.toml` sets a 120-column width. Run `cargo fmt --all` before committing.

## Testing

Write the failing test first, confirm it fails for the right reason, then the minimum code to pass, then the full suite. Parser tests use fixtures under `crates/ddo-etl/tests/fixtures`; API tests build an in-memory database. `cargo test --workspace`, `cargo lint`, and `cargo fmt --all --check` must all pass before committing.

## Dependencies

- `xtask` uses `ra-ap-rustc_lexer`, which asserts at compile time that `unicode-ident` and `unicode-properties` share a Unicode version. `Cargo.lock` pins `unicode-ident` to 1.0.24 until `unicode-properties` catches up; a bare `cargo update` reintroduces the mismatch. Bump both together.
- Docker is not installed on the maintainer's Mac; the image is only built by the deploy workflow.

## Commits

Plain imperative subjects, no prefixes. One logical change per commit, passing `cargo lint` and tests. Push to `main` directly; CI runs on every push and the deploy workflow runs weekly or on dispatch.
