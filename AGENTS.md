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

## Orientation

This is the data half of a two-repo project. The site is `ddo-tools`, a sibling directory under `~/Documents/Personal Projects/` (GitHub `WestonChan/ddo-tools`, live at https://ddo-tools.vercel.app, its own `AGENTS.md`). This API is live at https://ddo-data.fly.dev (Fly app `ddo-data`, region `ord`, suspends to zero between requests; `/v1/docs` is the OpenAPI UI and `/docs` redirects to the latest version, `/v1/version` reports the dataset SHA and row counts).

**Data flow.** Maetrim's DDOBuilderV2 `Output/DataFiles` (sparse checkout in `upstream/`, gitignored) → `ddo-etl build` → `ddo.db` (~14 MB SQLite, schema from `ddo-model`) → `ddo-api` serves it read-only with strong ETags → `ddo-tools` reads it over HTTP through `src/lib/api/`. Every response is immutable for a dataset version, so the frontend caches forever.

**Deploying.** Only through `.github/workflows/deploy.yml` (every push to `main` except Markdown-only ones, weekly schedule, or `workflow_dispatch` with `force`): it refreshes `upstream/`, skips if the live `/v1/version` already reports both this commit (`api_commit`, baked in at build time from `DDO_API_COMMIT`) and the upstream SHA, runs the ETL, checks row-count floors, builds the release binary, and runs `flyctl deploy --remote-only` with the `FLY_API_TOKEN` secret. The `Dockerfile` copies prebuilt artifacts, so `fly deploy` from a laptop does not work; `fly status -a ddo-data` is fine for inspection.

**Local API for the frontend.** `DDO_DB_PATH=ddo.db ICONS_DIR=icons PORT=8089 cargo run --release -p ddo-api`, then `VITE_API_URL=http://localhost:8089` in `ddo-tools/.env`. Icons come from `cargo run -p ddo-etl -- icons --source upstream/Output/DataFiles --out icons`.

**Where the API surface is.** `crates/ddo-api/src/routes/` has one file per resource (`items` with its slot, weapon, damage and socket vocabularies, `augments`, `sets` with filigrees and sentient gems, `feats`, `stances` for the standalone stance list, `buffs` for guild and optional buffs, `races`, `classes`, `enhancement_trees`, `spells`, `bonuses` for stats and bonus types, `quests` for quests, challenges, packs and patrons, `crafting` for the wiki crafting systems, `dump`, `version`); `db.rs` holds the shared query helpers; `etag.rs` and `state.rs` the caching and pool. The ETL mirrors it: `crates/ddo-etl/src/xml/` parses, `map/` applies `data/*.toml` vocabularies, `build/` writes tables.

**Planning.** The roadmap for both repos is `ddo-tools/docs/roadmap.md` (V-series section). V1–V6 are done; V7 fills what DDOBuilderV2 lacks (per-drop rarity, quests without named loot, crafting recipes) from ddowiki via agent-read overrides in `crates/ddo-etl/data/wiki/`, and V8 adds `/v1/builds` on a Fly volume for build sharing.

## Structure

- `crates/ddo-model` — the schema as Rust: enums, seeds, DDL, `SCHEMA_VERSION`. Based on ddo-tools' schema, not on DDOBuilderV2's XML; the ETL is the adapter.
- `crates/ddo-etl` — parses `Output/DataFiles` with quick-xml and writes SQLite: items, augments, sets and filigrees, sentient gems, feats, stances, races, classes, enhancement trees, spells, clickies, guild and optional buffs, quests, challenges and patrons. Only `AttackRates.xml`, `BonusTypes.xml`, `IgnoredList.xml` and `WeaponGroupings.xml` are unread. Mapping vocabularies are data in `data/*.toml`; an unmapped value fails the build on purpose.
- `crates/ddo-api` — axum read API over the ETL output. OpenAPI at `/v1/docs`, strong ETags, `X-Dataset-Version`, rate limiting, `/v1/dump.sqlite`, icons at `/icons/`.
- `xtask` — repo tooling behind `cargo xtask`. Not published.
- `upstream/` — gitignored sparse checkout of DDOBuilderV2; `.github/workflows/deploy.yml` refreshes it weekly and deploys to Fly.io.
- Planning lives in the ddo-tools roadmap (`docs/roadmap.md`, V-series section), not here.

**Wiki overrides.** `crates/ddo-etl/data/wiki/*.toml` holds facts read from ddowiki that Maetrim's files have no field for (so far, which quest drops are rare; each quest's duration, XP by tier and difficulty, and free-to-play status; and crafting systems' ingredients and the recipes that turn them into Maetrim's crafting augments). His files win for every field they carry; a wiki row only adds. Every row cites its page and read date, and the build fails on any name his files lack. The format, the validation rules and the command that checks a file are in [`crates/ddo-etl/data/wiki/README.md`](crates/ddo-etl/data/wiki/README.md); read it before adding or editing an entry.

## Code Quality

- **No comments, no doc comments.** Code carries its meaning in names, types, and tests. Do not write `//`, `/* */`, `///`, or `//!`. If something needs a comment to be understood, rename, split, or restructure it until it doesn't. Reasoning that cannot live in code goes in the ddo-tools roadmap or `docs/` there. `cargo lint` fails on any comment; `cargo xtask no-comments --fix` strips them.
- **Turn findings into lint rules.** When you fix or review a problem a machine could have caught (a convention broken in more than one place, a bug pattern, a rule in this file that nothing enforces), suggest a check that `cargo lint` runs so it can't come back: a clippy lint in `[workspace.lints]` or `clippy.toml`, or an `xtask` check. Say what it would flag today and include it in your final report. Add it yourself when it's cheap and needs no new dependency; a new crate needs the maintainer's approval first.
- **Names carry the meaning comments would have.** Follow the naming rules below.
- **Keep code clean and refactor freely.** Improve adjacent code you touch; don't leave a file worse than you found it.
- **`cargo lint` also enforces** `unreachable_pub` (so a crate-internal item is `pub(crate)` and dead code stays visible) and the naming lints in `[workspace.lints]` and `clippy.toml`: `disallowed_names` (`data`, `info`, `tmp`, `val` and similar), `many_single_char_names` (three or more single-letter bindings in one scope) and `similar_names`. Every crate opts in with `[lints] workspace = true`.
- `rustfmt.toml` sets a 120-column width. Run `cargo fmt --all` before committing.

### Naming

Names replace comments, so each name must answer the question a reader would otherwise ask. **Before naming or renaming anything, read `ddo-tools/docs/naming.md`** (the sibling checkout, or [on GitHub](https://github.com/WestonChan/ddo-tools/blob/main/docs/naming.md)). It covers how to choose a name, with examples and sources, and applies to both repos. Its examples use TypeScript casing; here the same words go in `snake_case` and `UpperCamelCase`. Existing code predates it and is not a model to copy; rename what you touch. The rules in short:

- **Choose, don't guess.** List the concepts the name must carry, pick one word per concept (the player's word, the same word the frontend and API use), put them in English order (`maximum_message_length`, `total_strength`, `cooldown_ms`), then read the call site on its own.
- **Code that changes data** is an imperative verb phrase naming what it changes, or why: `insert_quests`, `reject_unmapped_slot`. Never `update`, `process` or `handle`. A name that needs "and" means the function should be split.
- **Code that only reads, and every binding,** is a noun phrase naming what it is or what it's for, often through the process that made it: `sorted_items`, `rows_to_insert`. Reads have no side effects. Follow the Rust API Guidelines: no `get_` on getters; `as_`/`to_`/`into_` for conversions; `new`/`from_x` for constructors; `is_`/`has_`/`can_` for predicates.
- **Types are nouns**: the domain word if one exists (`Race`, `Augment`, `EquipmentSlot`), otherwise the role the thing plays. No `Manager`, `Helper`, `Info` or `Data`. Enum variants read as values (`Difficulty::Elite`).
- **A name means only one thing.** Qualify generic words (`slot`, `candidate`, `entry`, `child`, `kind`) until only one reading is left. Add words that remove ambiguity and drop words that repeat the type or the owner.
- **Name length follows scope**: short names only for values that live 10 lines or fewer. `ddo-model` and shared helpers get capability names, not a name from their first caller's point of view.

## API documentation

`/docs` is the public face of the API, and it is generated from the `#[utoipa::path]` attributes, so the attributes are the documentation. Every route carries `summary` (a short title in the form "List items" / "Get an item"), `description` (what the response contains and how the route behaves, naming the child collections), a `description` on every query and path parameter (what it matches, the vocabulary it comes from, the default), and a `description` on every response. Filters are declared inline in `params(...)` rather than through `IntoParams`, because the comment ban leaves no other place to describe a field. The `tags(...)` and `info(description)` on `ApiDoc` in `routes/v1/mod.rs` are the sidebar headings and the Introduction page. `openapi_describes_every_operation_parameter_and_tag` in `crates/ddo-api/tests/api.rs` fails the build when any of this is missing or too short, so adding a route means writing its docs in the same commit. When a response shape changes, reread its description.

Every JSON response also carries an example, attached after generation by `crates/ddo-api/src/docs.rs` from `crates/ddo-api/docs/examples/<route>.json`. The files are real responses from the live API, trimmed to three elements per array by `crates/ddo-api/docs/refresh-examples.sh` (pass a base URL to sample a local server instead). `openapi_carries_a_real_example_for_every_json_response` compares each example's keys and value types against a response from the fixture database, so a shape change fails until the examples are refreshed, and a new route fails until it has one.

**Versions.** The API is versioned by path prefix and every version owns its documentation: `routes/v1/` holds the handlers, the `ApiDoc` (info, tags) and the `RESPONSE_EXAMPLES` list, its examples live under `docs/examples/v1/`, and `lib.rs` mounts its spec at `/v1/openapi.json` and its UI at `/v1/docs`. `/docs`, `/openapi.json` and `/` redirect to `routes::LATEST_VERSION`. A breaking change (renaming or removing a field, changing a shape, changing what a filter matches) goes in a new `routes/v2/` beside `v1`, not in place: copy the module, change what must change, register it in `app()` next to v1, point `LATEST_VERSION` at it, and keep v1 serving until `ddo-tools` and any other consumer have moved. Additive changes (new field, new route, new optional filter) stay in the current version.

## Testing

Write the failing test first, confirm it fails for the right reason, then the minimum code to pass, then the full suite. Parser tests use fixtures under `crates/ddo-etl/tests/fixtures`; API tests build an in-memory database. `cargo test --workspace`, `cargo lint`, and `cargo fmt --all --check` must all pass before committing.

## Dependencies

- `xtask` uses `ra-ap-rustc_lexer`, which asserts at compile time that `unicode-ident` and `unicode-properties` share a Unicode version. `Cargo.lock` pins `unicode-ident` to 1.0.24 until `unicode-properties` catches up; a bare `cargo update` reintroduces the mismatch. Bump both together.
- Docker is not installed on the maintainer's Mac; the image is only built by the deploy workflow.

## Commits

Plain imperative subjects, no prefixes. One logical change per commit, passing `cargo lint` and tests. Push to `main` directly; CI runs on every push and the deploy workflow runs weekly or on dispatch.
