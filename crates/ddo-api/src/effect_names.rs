use rusqlite::{Connection, OptionalExtension};

pub(crate) struct ResolvedEffectName {
    pub(crate) effect_id: i64,
    pub(crate) alias_name: Option<String>,
}

pub(crate) fn resolve_effect_name(db: &Connection, text: &str) -> rusqlite::Result<Option<ResolvedEffectName>> {
    db.query_row(
        "SELECT effect_id, alias_name FROM effect_names
          WHERE (name = ?1 OR slug = ?1) AND (alias_name IS NULL OR slug IS NOT NULL)
          ORDER BY alias_order LIMIT 1",
        [text],
        |row| Ok(ResolvedEffectName { effect_id: row.get(0)?, alias_name: row.get(1)? }),
    )
    .optional()
}

const EFFECT_NAME_SEARCH_SQL: &str = "EXISTS (
    SELECT 1 FROM effect_names searched WHERE searched.effect_id = {effect_id}
       AND (searched.alias_name IS NULL OR searched.slug IS NOT NULL)
       AND searched.name LIKE '%' || {query} || '%' ESCAPE '\\')";

pub(crate) fn effect_name_search_sql(effect_id: &str, query: &str) -> String {
    EFFECT_NAME_SEARCH_SQL.replace("{effect_id}", effect_id).replace("{query}", query)
}

pub(crate) fn matched_alias_sql(effect_id: &str, query: Option<&str>) -> String {
    let Some(query) = query else { return "NULL".into() };
    format!(
        "(SELECT matched.alias_name FROM effect_names matched
               WHERE matched.effect_id = {effect_id} AND (matched.alias_name IS NULL OR matched.slug IS NOT NULL)
                 AND matched.name LIKE '%' || {query} || '%' ESCAPE '\\'
               ORDER BY matched.alias_order LIMIT 1)"
    )
}
