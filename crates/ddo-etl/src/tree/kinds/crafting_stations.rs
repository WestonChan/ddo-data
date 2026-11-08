use super::crafting_system::{CraftingUnlock, CraftingUnlockOwner};
use anyhow::{Context, Result};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingStation in "crafting_stations" {
        keys { id: i64, slug: &str, sort_order: i64 }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            kind: ddo_model::enums::StationKind => kind [enum_text],
            #[serde(default, skip_serializing_if = "Option::is_none")]
            place: Option<String> => place,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            quest: Option<String> => quest_id [optional_reference "quests"],
        }
        attached {
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            unlock: Vec<CraftingUnlock>,
            citation: Vec<crate::tree::Citation>,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CraftingStationsFile {
    pub(crate) row: Vec<CraftingStation>,
}

impl CraftingStationsFile {
    pub(in crate::tree) fn write_stations(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut crate::tree::ids::StableIds,
    ) -> Result<()> {
        for (position, station) in self.row.iter().enumerate() {
            let identity = crate::tree::qualified_slug(&station.name, station.variant.as_deref());
            let id = ids.issue("crafting_stations", &identity, path)?;
            station
                .insert(db, id, &identity, position as i64)
                .with_context(|| format!("{}: station {:?}", path.display(), station.name))?;
        }
        Ok(())
    }

    pub(in crate::tree) fn insert_unlocks(&self, db: &Connection) -> Result<()> {
        for station in &self.row {
            let id = crate::tree::crafting::qualified_reference_id(
                db,
                "crafting_stations",
                &station.name,
                station.variant.as_deref(),
            )?;
            for (position, unlock) in station.unlock.iter().enumerate() {
                unlock.write_unlock(db, CraftingUnlockOwner::Station(id), position as i64)?;
            }
        }
        Ok(())
    }

    pub(in crate::tree) fn from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<Self> {
        let mut stations = Vec::new();
        let mut locations = BTreeMap::new();
        let mut query = db.prepare(&format!(
            "SELECT {} FROM crafting_stations ORDER BY sort_order,id",
            CraftingStation::select_columns()
        ))?;
        for row in query.query_map([], |row| Ok((row.get::<_, i64>("id")?, CraftingStation::from_row(row)?)))? {
            let (id, mut station) = row?;
            station.attach_annotations(annotations, id);
            locations.insert(id, stations.len());
            stations.push(station);
        }
        let mut query = db.prepare(&format!(
            "SELECT {} FROM crafting_unlocks WHERE station_id IS NOT NULL ORDER BY station_id,sort_order",
            CraftingUnlock::select_columns()
        ))?;
        for row in query.query_map([], |row| Ok((row.get::<_, i64>("station_id")?, CraftingUnlock::from_row(row)?)))? {
            let (id, unlock) = row?;
            stations[locations[&id]].unlock.push(unlock);
        }
        Ok(Self { row: stations })
    }
}
