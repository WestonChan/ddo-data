use anyhow::{ensure, Result};
use std::collections::BTreeMap;
use std::path::Path;

const MAX_JSON_INTEGER: u64 = (1 << 53) - 1;

pub(super) fn stable_hash(parts: &[&str]) -> i64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in (part.len() as u64).to_le_bytes().iter().chain(part.as_bytes()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    ((hash & MAX_JSON_INTEGER).max(1)) as i64
}

#[derive(Default)]
pub(super) struct StableIds {
    identities_by_id: BTreeMap<(String, i64), (String, std::path::PathBuf)>,
}

impl StableIds {
    pub(super) fn issue(&mut self, kind: &str, identity: &str, path: &Path) -> Result<i64> {
        ensure!(!identity.is_empty(), "{}: {kind} has an empty slug", path.display());
        let id = stable_hash(&[kind, identity]);
        let key = (kind.to_string(), id);
        if let Some((previous, previous_path)) =
            self.identities_by_id.insert(key, (identity.to_string(), path.to_path_buf()))
        {
            ensure!(
                previous != identity,
                "duplicate {kind} slug {identity:?}{}",
                if previous_path == path { String::new() } else { format!(" (also in {})", previous_path.display()) }
            );
            anyhow::bail!(
                "{} and {}: {kind} hash collision between {previous:?} and {identity:?}",
                previous_path.display(),
                path.display()
            );
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_ids_fit_json_integers_and_follow_names() {
        let first = stable_hash(&["items", "test-ring"]);
        let renamed = stable_hash(&["items", "renamed-ring"]);
        assert!(first > 0 && first <= MAX_JSON_INTEGER as i64);
        assert_ne!(first, renamed);
        assert_eq!(first, stable_hash(&["items", "test-ring"]));
    }

    #[test]
    fn collision_check_names_both_rows() {
        let mut ids = StableIds::default();
        ids.issue("items", "test-ring", Path::new("first/ring.toml")).unwrap();
        let error = ids.issue("items", "test-ring", Path::new("second/ring.toml")).unwrap_err().to_string();
        assert!(error.contains("duplicate items slug"), "{error}");
        assert!(error.contains("first/ring.toml"), "{error}");
        assert!(error.contains("second/ring.toml"), "{error}");
    }
}
