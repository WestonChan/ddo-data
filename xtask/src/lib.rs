pub mod dataset;
pub mod golden_refresh;
pub mod integrity;
pub mod response_examples;
pub mod wiki_tools;
pub mod wiki_tooltips;

use std::path::{Path, PathBuf};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}
pub mod api_validation;
