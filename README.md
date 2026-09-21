# ddo-data

[![CI](https://github.com/WestonChan/ddo-data/actions/workflows/ci.yml/badge.svg)](https://github.com/WestonChan/ddo-data/actions/workflows/ci.yml)
[![Last commit](https://img.shields.io/github/last-commit/WestonChan/ddo-data)](https://github.com/WestonChan/ddo-data/commits/main)
[![License](https://img.shields.io/github/license/WestonChan/ddo-data)](LICENSE)

Game-data pipeline and public API for [DDO Tools](https://github.com/WestonChan/ddo-tools), a
build and gear planner for [Dungeons & Dragons Online](https://www.ddo.com/).

Game data is parsed from [Maetrim's DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2) data
files into a SQLite database, then served read-only by an [axum](https://github.com/tokio-rs/axum)
server on [Fly.io](https://fly.io/). The frontend at `ddo-tools` and any other consumer read the
same versioned API.

## Crates

This is a Cargo workspace with three crates.

| Crate | Role |
|---|---|
| `ddo-model` | The DDO Tools database schema as Rust types: row structs, closed-vocabulary enums, DDL, and the dataset version. No I/O. |
| `ddo-etl` | Parses DDOBuilderV2's XML data files into that schema and writes SQLite. Runs offline as a build step. |
| `ddo-api` | Axum server exposing the SQLite read-only as a versioned public API with an OpenAPI spec. |

The ETL and the API share the schema and ship together in one Docker image, which is why they live
in one repository. The plan, phase status and design decisions live in the DDO Tools roadmap under
its "V-series" section.

## Getting Started

Requires a stable Rust toolchain (`rustup`). `rust-toolchain.toml` pins the channel and components.

```bash
cargo test
```

The ETL needs a sparse checkout of DDOBuilderV2 limited to its data files (~87 MB). It is ignored
by git.

```bash
git clone --depth 1 --filter=blob:none --sparse https://github.com/Maetrim/DDOBuilderV2 upstream
git -C upstream sparse-checkout set Output/DataFiles
cargo run -p ddo-etl -- build --source upstream/Output/DataFiles --out ddo.db
```

### Available Commands

| Command | Description |
|---|---|
| `cargo test --workspace` | Run every crate's tests |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint, as CI runs it |
| `cargo fmt --all --check` | Formatting check, as CI runs it |
| `cargo run -p ddo-etl -- build --source <DataFiles> --out ddo.db` | Build the game database |
| `cargo run -p ddo-etl -- diff --db ddo.db --legacy <old ddo.db>` | Compare item coverage against a previous database |
| `DDO_DB_PATH=ddo.db cargo run -p ddo-api` | Serve the API on http://localhost:8080 (docs at `/docs`, spec at `/openapi.json`) |

## API

`ddo-api` serves the database read-only. Every response carries a strong `ETag`, a long
`Cache-Control`, and an `X-Dataset-Version` header naming the DDOBuilderV2 commit, so browsers and
CDNs can cache aggressively and revalidate with `If-None-Match`. Responses are gzip/brotli
compressed, CORS allows any origin for `GET`, and requests are rate limited per IP. For bulk access
download the whole database once from `/v1/dump.sqlite` rather than paging the list endpoints.

| Path | What |
|---|---|
| `/v1/version` | Dataset and schema versions, table counts |
| `/v1/items`, `/v1/items/{id}` | Equipment, filterable by name, slot, category, level, pack, raid, stat |
| `/v1/augments`, `/v1/augments/{id}` | Augments with the sockets they fit |
| `/v1/sets`, `/v1/sets/{id}`, `/v1/filigrees` | Gear sets, filigree sets, filigrees |
| `/v1/feats`, `/v1/feats/{id}` | Feats from the standard list, classes and races |
| `/v1/races`, `/v1/classes` (+ `/{id}`) | Races and classes |
| `/v1/enhancement-trees`, `/v1/enhancement-trees/{id}` | Trees with every enhancement and selection |
| `/v1/spells`, `/v1/spells/{id}`, `/v1/clickies` | Spells and item clickies |
| `/v1/stats`, `/v1/bonus-types`, … | Reference vocabularies |
| `/v1/dump.sqlite` | The whole database |
| `/docs`, `/openapi.json` | Interactive documentation and the OpenAPI 3.1 spec |

## Deployment

A scheduled GitHub Action pulls DDOBuilderV2, runs the ETL, validates the result, and deploys the
API image to Fly.io (`fly.toml`, `Dockerfile`). The database is baked into the image, so every
deploy is an immutable dataset and Fly's release history doubles as the dataset history. The
machine suspends between requests and resumes on the next one.

## Credits

- [Maetrim's DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2) -- the source of the game data
  (items, feats, enhancement trees, classes, races, quests, set bonuses, augments, spells, icons),
  used with the author's permission. The repository carries no license file; that permission is
  the basis for use.
- [DDO Wiki](https://ddowiki.com/) -- the source of the DDO Tools database this schema descends
  from, and the intended second source once its API is reachable again; wiki content is available
  under CC BY-SA.
- [DDO Tools](https://github.com/WestonChan/ddo-tools) -- the Python pipeline this project replaces,
  whose database is the correctness fixture the ETL is checked against.

## License

[MIT](LICENSE)

ddo-data is an unaffiliated fan project. Dungeons & Dragons Online is © Standing Stone Games; game
content, names, imagery, and data belong to their respective owners. The MIT license covers this
repository's code, not the game data it parses or serves.
