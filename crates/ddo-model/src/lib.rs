//! The DDO Tools database schema as Rust types.
//!
//! This crate is the contract between `ddo-etl`, which writes the database, and `ddo-api`, which
//! serves it. It holds the closed vocabularies ([`enums`]), the seed tables ([`seeds`], [`stats`])
//! and the DDL ([`ddl`]). It does no parsing and no HTTP.

pub mod enums;
pub mod schema;
pub mod seeds;
pub mod stats;

pub use schema::{ddl, SCHEMA_VERSION};

/// Find a stat by its canonical name.
pub fn stat_by_name(name: &str) -> Option<&'static stats::Stat> {
    stats::STATS.iter().find(|s| s.name == name)
}

/// Which upstream DDOBuilderV2 commit a database was built from, and when.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DatasetVersion {
    pub upstream_sha: String,
    pub built_at: String,
}
