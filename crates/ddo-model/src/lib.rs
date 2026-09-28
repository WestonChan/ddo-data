pub mod enums;
pub mod schema;
pub mod seeds;
pub mod stats;

pub use schema::{ddl, SCHEMA_VERSION};

pub fn stat_by_name(name: &str) -> Option<&'static stats::Stat> {
    stats::STATS.iter().find(|s| s.name == name)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DatasetVersion {
    pub upstream_sha: String,
    pub built_at: String,
}
