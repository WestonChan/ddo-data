pub mod data_file_lints;
pub mod dataset;
pub mod diff_db;
pub mod golden_refresh;
pub mod integrity;
pub mod redirect_checks;
pub mod response_examples;
pub mod round_trip;
pub mod wiki_tools;
pub mod wiki_tooltips;

use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
pub mod api_validation;
