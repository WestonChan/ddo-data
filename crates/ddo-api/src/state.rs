//! Shared state: a small pool of read-only SQLite connections and the dataset version.

use crate::error::ApiError;
use anyhow::{Context, Result};
use ddo_model::DatasetVersion;
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const POOL_SIZE: usize = 8;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<Inner>,
}

struct Inner {
    path: PathBuf,
    pool: Mutex<Vec<Connection>>,
    dataset: DatasetVersion,
    schema_version: i64,
    rate_limited: bool,
}

impl AppState {
    /// Open the database read-only and read its version rows.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = open_connection(path)?;
        let dataset = conn
            .query_row("SELECT upstream_sha, built_at FROM dataset_version", [], |r| {
                Ok(DatasetVersion { upstream_sha: r.get(0)?, built_at: r.get(1)? })
            })
            .context("reading dataset_version")?;
        let schema_version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .context("reading schema_version")?;
        Ok(Self {
            inner: Arc::new(Inner {
                path: path.to_path_buf(),
                pool: Mutex::new(vec![conn]),
                dataset,
                schema_version,
                rate_limited: false,
            }),
        })
    }

    /// Turn on per-IP rate limiting. Needs a real peer address, so tests leave it off.
    pub fn with_rate_limit(mut self) -> Self {
        Arc::get_mut(&mut self.inner).expect("state not yet shared").rate_limited = true;
        self
    }

    pub fn rate_limited(&self) -> bool {
        self.inner.rate_limited
    }

    pub fn dataset(&self) -> &DatasetVersion {
        &self.inner.dataset
    }

    pub fn schema_version(&self) -> i64 {
        self.inner.schema_version
    }

    pub fn db_path(&self) -> &Path {
        &self.inner.path
    }

    /// Run a blocking query on a pooled connection.
    pub async fn query<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> Result<T, ApiError> + Send + 'static,
    {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || {
            let conn = match inner.pool.lock().expect("pool lock").pop() {
                Some(c) => c,
                None => open_connection(&inner.path).map_err(ApiError::from)?,
            };
            let result = f(&conn);
            let mut pool = inner.pool.lock().expect("pool lock");
            if pool.len() < POOL_SIZE {
                pool.push(conn);
            }
            result
        })
        .await
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("query task failed: {e}")))?
    }
}

fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .with_context(|| format!("opening {}", path.display()))?;
    conn.execute_batch("PRAGMA query_only = 1;")?;
    Ok(conn)
}
