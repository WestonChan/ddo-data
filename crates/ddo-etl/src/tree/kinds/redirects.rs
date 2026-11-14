use anyhow::{ensure, Result};
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub struct Redirect in "redirects" {
        keys {}
        fields {
            kind: EntityKind => kind [enum_text],
            from: String => from_slug,
            to: String => to_slug,
            since: String => since,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RedirectsFile {
    #[serde(default)]
    pub(crate) redirect: Vec<Redirect>,
}

impl RedirectsFile {
    pub(in crate::tree) fn write_redirects(&self, db: &Connection) -> Result<()> {
        let mut targets = BTreeMap::new();
        let mut violations = Vec::new();
        for redirect in &self.redirect {
            if let Some(previous) = targets.insert((redirect.kind.as_str(), &redirect.from), &redirect.to) {
                violations.push(format!(
                    "duplicate redirect {} {:?}: to {previous:?} and {:?}",
                    redirect.kind.as_str(),
                    redirect.from,
                    redirect.to
                ));
            }
        }
        ensure!(violations.is_empty(), "{}", violations.join("\n"));
        for redirect in &self.redirect {
            redirect.insert(db).map_err(|error| {
                anyhow::anyhow!("redirect {} {:?} -> {:?}: {error}", redirect.kind.as_str(), redirect.from, redirect.to)
            })?;
        }
        Ok(())
    }

    pub(in crate::tree) fn from_rows(db: &Connection) -> Result<Self> {
        let redirect = db
            .prepare(&format!("SELECT {} FROM redirects ORDER BY kind, from_slug", Redirect::select_columns()))?
            .query_map([], Redirect::from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { redirect })
    }
}
