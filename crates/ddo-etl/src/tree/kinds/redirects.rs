use anyhow::{ensure, Result};
use ddo_model::enums::EntityKind;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Redirect in "redirects" {
        keys {}
        fields {
            kind: EntityKind => kind [enum_text],
            from: String => from_slug,
            to: String => to_slug,
            since: String => since,
        }
    }
}

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct RemovedSlug in "removed_slugs" {
        keys {}
        fields {
            kind: EntityKind => kind [enum_text],
            slug: String => slug,
            since: String => since,
            reason: String => reason,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RedirectsFile {
    #[serde(default)]
    pub(crate) redirect: Vec<Redirect>,
    #[serde(default)]
    pub(crate) removed: Vec<RemovedSlug>,
}

impl RedirectsFile {
    pub(in crate::tree) fn write_redirects(&self, db: &Connection) -> Result<()> {
        let mut targets = BTreeMap::new();
        let mut removals = BTreeMap::new();
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
        for removed in &self.removed {
            if let Some(previous) = removals.insert((removed.kind.as_str(), &removed.slug), &removed.reason) {
                violations.push(format!(
                    "duplicate removed {} {:?}: {previous:?} and {:?}",
                    removed.kind.as_str(),
                    removed.slug,
                    removed.reason
                ));
            }
        }
        ensure!(violations.is_empty(), "{}", violations.join("\n"));
        for redirect in &self.redirect {
            redirect.insert(db).map_err(|error| {
                anyhow::anyhow!("redirect {} {:?} -> {:?}: {error}", redirect.kind.as_str(), redirect.from, redirect.to)
            })?;
        }
        for removed in &self.removed {
            removed.insert(db).map_err(|error| {
                anyhow::anyhow!("removed {} {:?} ({:?}): {error}", removed.kind.as_str(), removed.slug, removed.reason)
            })?;
        }
        Ok(())
    }

    pub(in crate::tree) fn from_rows(db: &Connection) -> Result<Self> {
        let redirect = db
            .prepare(&format!("SELECT {} FROM redirects ORDER BY kind, from_slug", Redirect::select_columns()))?
            .query_map([], Redirect::from_row)?
            .collect::<rusqlite::Result<_>>()?;
        let removed = db
            .prepare(&format!("SELECT {} FROM removed_slugs ORDER BY kind, slug", RemovedSlug::select_columns()))?
            .query_map([], RemovedSlug::from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Self { redirect, removed })
    }
}
