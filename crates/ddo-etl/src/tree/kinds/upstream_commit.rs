database_record! {
    #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct UpstreamCommitFile in "dataset_version" {
        keys { built_at: &str }
        fields { sha: String => upstream_sha }
    }
}

impl UpstreamCommitFile {
    pub(in crate::tree) fn from_rows(db: &rusqlite::Connection) -> anyhow::Result<Self> {
        Ok(db.query_row(&format!("SELECT {} FROM dataset_version", Self::select_columns()), [], Self::from_row)?)
    }

    pub(in crate::tree) fn write_dataset_version(
        &self,
        db: &rusqlite::Connection,
        root: &std::path::Path,
    ) -> anyhow::Result<()> {
        let version = crate::upstream::dataset_version(root, Some(self.sha.clone()));
        self.insert(db, &version.built_at)?;
        Ok(())
    }
}
