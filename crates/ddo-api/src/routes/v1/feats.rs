use crate::db::{
    attack_for, bonuses_via, convert_to_booleans, dcs_for, json_row, json_rows, modifiers_for, paged_query,
    requirements_for, stances_for, WhereClause,
};
use crate::error::ApiError;
use crate::query::{declare_list_parameters, declare_query_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use ddo_model::enums::FeatSource;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(feats)).routes(routes!(feat_detail))
}

declare_query_parameters! {
    pub(super) struct FeatFilters {
        pub source: Option<String>,
        pub group: Option<String>,
        pub acquire: Option<String>,
    }
}

const FEAT_COLUMNS: &str = "f.id, f.name, f.source_kind, f.source_id,
                       CASE f.source_kind WHEN 'class' THEN (SELECT c.name FROM classes c WHERE c.id = f.source_id)
                                          WHEN 'race' THEN (SELECT r.name FROM races r WHERE r.id = f.source_id) END AS source_name,
                       f.description, f.icon, f.acquire, f.max_times_acquire, f.sphere, f.auto_acquire_ignores_requirements";

const FEATS_SORT_FIELDS: &[(&str, &str)] = &[
    ("name", "f.name"),
    ("id", "f.id"),
    ("source_kind", "f.source_kind"),
    ("acquire", "f.acquire"),
    ("sphere", "f.sphere"),
    ("auto_acquire_ignores_requirements", "auto_acquire_ignores_requirements"),
    ("description", "description"),
    ("icon", "icon"),
    ("max_times_acquire", "max_times_acquire"),
    ("source_id", "source_id"),
    ("source_name", "source_name"),
    ("source", "f.source_kind"),
    ("group", "(SELECT MIN(fg.group_name) FROM feat_groups fg WHERE fg.feat_id = f.id)"),
];

declare_list_parameters!(FeatsParameters, FEATS_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/feats",
    tag = "feats",
    summary = "List feats",
    description = "Lists feats with sources, acquisition rules and groups.",
    params(
        FeatsParameters,
        ("source" = Option<String>, Query, description = "`standard`, `class` or `race`; anything else is a 400"),
        ("group" = Option<String>, Query, description = "Feat group name, e.g. `Metamagic`; keeps feats listed in that group"),
        ("acquire" = Option<String>, Query, description = "How the feat is taken, e.g. `Train`, `Automatic`, `Special`"),
    ),
    responses(
        (status = 200, description = "`total`, `limit`, `offset` and the `feats` page", body = crate::routes::v1::response_schemas::FeatsPageResponse),
        (status = 400, description = "Unknown source, or an unknown or malformed query parameter", body = crate::error::ErrorBody)
    )
)]
async fn feats(
    State(state): State<AppState>,
    ApiQuery(query, filters): ApiQuery<FeatFilters>,
) -> Result<Json<Value>, ApiError> {
    if let Some(source) = &filters.source {
        if !FeatSource::ALL.iter().any(|known| known.as_str() == source) {
            return Err(ApiError::BadRequest(format!("unknown source {source:?}")));
        }
    }
    state
        .read_db(move |db| {
            let mut where_clause = WhereClause::default();
            where_clause.add_name_search(query.q.as_deref(), "f.name");
            if let Some(source) = &filters.source {
                where_clause.add_bound_condition("f.source_kind = ?", source.clone());
            }
            if let Some(group) = &filters.group {
                where_clause.add_bound_condition(
                    "EXISTS (SELECT 1 FROM feat_groups fg WHERE fg.feat_id = f.id AND fg.group_name = ?)",
                    group.clone(),
                );
            }
            if let Some(acquire) = &filters.acquire {
                where_clause.add_bound_condition("f.acquire = ?", acquire.clone());
            }
            let mut page = paged_query(
                db,
                FEAT_COLUMNS,
                "feats f",
                &query,
                "f.name, f.source_kind",
                FEATS_SORT_FIELDS,
                &where_clause,
            )?;
            for feat in &mut page.rows {
                convert_to_booleans(feat, &["auto_acquire_ignores_requirements"]);
                let feat_id = feat["id"].as_i64().unwrap_or(0);
                feat["groups"] = Value::Array(feat_group_names(db, feat_id)?);
            }
            Ok(Json(page.into_json("feats")))
        })
        .await
}

fn feat_group_names(db: &rusqlite::Connection, feat_id: i64) -> Result<Vec<Value>, ApiError> {
    Ok(json_rows(db, "SELECT group_name FROM feat_groups WHERE feat_id = ?1 ORDER BY rowid", [feat_id])?
        .into_iter()
        .map(|row| row["group_name"].clone())
        .collect())
}

#[utoipa::path(
    get,
    path = "/v1/feats/{id}",
    tag = "feats",
    summary = "Get a feat",
    description = "Returns a feat with requirements, bonuses, attacks and modifiers.",
    params(("id" = i64, Path, description = "The feat's numeric id from the list endpoint")), responses((status = 200, description = "The feat with its child collections", body = crate::routes::v1::response_schemas::FeatsDetailResponse), (status = 404, description = "No feat has this id", body = crate::error::ErrorBody))
)]
async fn feat_detail(State(state): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let mut feat = json_row(db, &format!("SELECT {FEAT_COLUMNS} FROM feats f WHERE f.id = ?1"), [id])?;
            convert_to_booleans(&mut feat, &["auto_acquire_ignores_requirements"]);
            feat["groups"] = Value::Array(feat_group_names(db, id)?);
            feat["requirements"] = Value::Array(requirements_for(db, "feat", id)?);
            feat["auto_acquire_requirements"] = Value::Array(requirements_for(db, "feat_auto_acquire", id)?);
            let mut conditional_groups =
                json_rows(db, "SELECT id, groups FROM feat_conditional_groups WHERE feat_id = ?1", [id])?;
            for conditional_group in &mut conditional_groups {
                let conditional_group_id = conditional_group["id"].as_i64().unwrap_or(0);
                conditional_group["requirements"] =
                    Value::Array(requirements_for(db, "feat_conditional_group", conditional_group_id)?);
            }
            feat["conditional_groups"] = Value::Array(conditional_groups);
            feat["sub_items"] = Value::Array(json_rows(
                db,
                "SELECT name, icon, description FROM feat_sub_items WHERE feat_id = ?1 ORDER BY sort_order",
                [id],
            )?);
            feat["stances"] = Value::Array(stances_for(db, "feat", id)?);
            feat["dcs"] = Value::Array(dcs_for(db, "feat", id)?);
            feat["attack"] = attack_for(db, "feat", id)?;
            feat["this_attack_modifiers"] = Value::Array(modifiers_for(db, "feat_this_attack", id)?);
            feat["follow_on_modifiers"] = Value::Array(modifiers_for(db, "feat_follow_on", id)?);
            feat["bonuses"] = Value::Array(bonuses_via(db, "feat_enchantments", "feat_id", id)?);
            feat["modifiers"] = Value::Array(modifiers_for(db, "feat", id)?);
            Ok(Json(feat))
        })
        .await
}
