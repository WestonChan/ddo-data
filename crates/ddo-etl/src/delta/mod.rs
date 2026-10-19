mod compute;
mod identity;
mod merge;
mod table_merges;

pub use compute::{database_delta, ArrayChange, ArrayKey, ColumnChange, DatabaseDelta, EntityTableDelta};
pub use identity::{polymorphic_table, scalar_text, sql_text, IdentityIndex, RowFields};
pub(crate) use merge::clear_rederived_tables;
pub use merge::{
    merge_delta_into, AbsentChange, AppliedCounts, Conflict, ConflictSubject, DeclinedRemoval, DeletedEntity,
    MergeOutcome, RemovalDeclineReason,
};
pub(crate) use table_merges::{is_ours_only_column, is_rebuilt_column};
pub use table_merges::{table_merge, TableMerge, OURS_ONLY_COLUMNS, REBUILT_COLUMNS, TABLE_MERGES};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod merge_tests;
