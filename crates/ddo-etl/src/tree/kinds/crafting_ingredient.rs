use anyhow::Result;
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

database_record! {
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct CraftingIngredientFile in "crafting_ingredients" {
        keys { id: i64, slug: &str }
        fields {
            name: String => name,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            variant: Option<String> => variant,
            tier: String => tier,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            bind: Option<String> => bind,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            drop_location: Option<String> => source,
        }
        attached { citation: Vec<crate::tree::Citation> }
    }
}

impl CraftingIngredientFile {
    pub(in crate::tree) fn write_ingredient(
        &self,
        db: &Connection,
        path: &Path,
        ids: &mut crate::tree::ids::StableIds,
    ) -> Result<i64> {
        let identity = crate::tree::qualified_slug(&self.name, self.variant.as_deref());
        let id = ids.issue("crafting_ingredients", &identity, path)?;
        self.insert(db, id, &identity)?;
        Ok(id)
    }

    pub(in crate::tree) fn all_from_rows(
        db: &Connection,
        annotations: &crate::tree::annotations::EntityAnnotations,
    ) -> Result<BTreeMap<i64, Self>> {
        let mut query = db.prepare(&format!(
            "SELECT {} FROM crafting_ingredients WHERE system_id IS NULL ORDER BY id",
            Self::select_columns()
        ))?;
        let ingredients = query
            .query_map([], |row| {
                let id = row.get::<_, i64>("id")?;
                let mut ingredient = Self::from_row(row)?;
                ingredient.attach_annotations(annotations, id);
                Ok((id, ingredient))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ingredients)
    }
}
