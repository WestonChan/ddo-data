use crate::error::ApiError;
use anyhow::{Context, Result};
use ddo_model::DatasetVersion;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const POOL_CAPACITY: usize = 8;

#[derive(Clone)]
pub struct AppState {
    shared: Arc<SharedAppState>,
}

struct SharedAppState {
    db_path: PathBuf,
    pool: Mutex<Vec<Connection>>,
    dataset_version: DatasetVersion,
    schema_version: i64,
    is_rate_limited: bool,
    icons_dir: Option<PathBuf>,
    api_commit: Option<String>,
}

impl AppState {
    pub fn open(db_path: &Path) -> Result<Self> {
        let db = open_read_only_db(db_path)?;
        let dataset_version = db
            .query_row("SELECT upstream_sha, built_at FROM dataset_version", [], |row| {
                Ok(DatasetVersion { upstream_sha: row.get(0)?, built_at: row.get(1)? })
            })
            .context("reading dataset_version")?;
        let schema_version: i64 = db
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| row.get(0))
            .context("reading schema_version")?;
        Ok(Self {
            shared: Arc::new(SharedAppState {
                db_path: db_path.to_path_buf(),
                pool: Mutex::new(vec![db]),
                dataset_version,
                schema_version,
                is_rate_limited: false,
                icons_dir: None,
                api_commit: option_env!("DDO_API_COMMIT").map(str::to_string),
            }),
        })
    }

    pub fn with_rate_limit(mut self) -> Self {
        Arc::get_mut(&mut self.shared).expect("state not yet shared").is_rate_limited = true;
        self
    }

    pub fn with_icons_dir(mut self, icons_dir: &Path) -> Self {
        Arc::get_mut(&mut self.shared).expect("state not yet shared").icons_dir = Some(icons_dir.to_path_buf());
        self
    }

    pub fn with_api_commit(mut self, api_commit: &str) -> Self {
        Arc::get_mut(&mut self.shared).expect("state not yet shared").api_commit = Some(api_commit.to_string());
        self
    }

    pub fn api_commit(&self) -> Option<&str> {
        self.shared.api_commit.as_deref()
    }

    pub fn icons_dir(&self) -> Option<&Path> {
        self.shared.icons_dir.as_deref()
    }

    pub fn is_rate_limited(&self) -> bool {
        self.shared.is_rate_limited
    }

    pub fn dataset_version(&self) -> &DatasetVersion {
        &self.shared.dataset_version
    }

    pub fn schema_version(&self) -> i64 {
        self.shared.schema_version
    }

    pub fn db_path(&self) -> &Path {
        &self.shared.db_path
    }

    pub async fn read_db<T, F>(&self, read: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, ApiError> + Send + 'static,
    {
        let shared = self.shared.clone();
        tokio::task::spawn_blocking(move || {
            let db = match shared.pool.lock().expect("pool lock").pop() {
                Some(pooled_db) => pooled_db,
                None => open_read_only_db(&shared.db_path).map_err(ApiError::from)?,
            };
            let read_result = read(&db);
            let mut pool = shared.pool.lock().expect("pool lock");
            if pool.len() < POOL_CAPACITY {
                pool.push(db);
            }
            read_result
        })
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("query task failed: {e}")))?
    }
}

fn open_read_only_db(db_path: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("opening {}", db_path.display()))?;
    db.execute_batch("PRAGMA query_only = 1;")?;
    Ok(db)
}
