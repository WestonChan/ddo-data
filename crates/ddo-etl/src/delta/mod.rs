mod compute;
mod identity;
mod table_merges;

pub use compute::{database_delta, ArrayChange, ArrayKey, ColumnChange, DatabaseDelta, EntityTableDelta};
pub use identity::{polymorphic_table, scalar_text, sql_text, IdentityIndex, RowFields};
pub use table_merges::{table_merge, TableMerge, REBUILT_COLUMNS, TABLE_MERGES};

#[cfg(test)]
mod tests;
