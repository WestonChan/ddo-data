//! The adapter from DDOBuilderV2's XML data files onto the DDO Tools schema.
//!
//! Three layers, in dependency order:
//! - [`xml`] deserialises upstream files into structs that mirror *their* grammar exactly.
//! - [`map`] translates their vocabulary onto ours (`ddo-model`). Every mapping is a data file
//!   under `data/`, and a value we have no mapping for is an error, never a silent NULL.
//! - [`build`] walks a `DataFiles` directory and writes the database; [`diff`] checks the result
//!   against a previous database.

pub mod build;
pub mod diff;
pub mod icons;
pub mod map;
pub mod xml;
