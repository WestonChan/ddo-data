pub mod enchantment_amount;
pub mod enums;
pub mod schema;
pub mod seeds;
pub mod stats;

pub use schema::{ddl, SCHEMA_VERSION};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DatasetVersion {
    pub upstream_sha: String,
    pub built_at: String,
}
