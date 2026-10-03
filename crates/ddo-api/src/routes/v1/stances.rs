use crate::db::paged_stances_for;
use crate::error::ApiError;
use crate::query::{declare_list_parameters, ApiQuery};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ddo_model::enums::AbilityOwner;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(stances))
}

const STANCES_SORT_FIELDS: &[(&str, &str)] = &[("name", "name"), ("id", "id"), ("group_name", "group_name")];

declare_list_parameters!(StancesParameters, STANCES_SORT_FIELDS, "");

#[utoipa::path(
    get,
    path = "/v1/stances",
    tag = "stances",
    summary = "List stances",
    description = "The standalone stances from DDOBuilderV2's stance list (Two Weapon Fighting, Heavy Armor, Aura \
                   of Good, ...) in the order the planner shows them, each with its icon, description, `group_name`, \
                   whether the planner turns it on automatically (`auto_controlled`), the stances it cannot be \
                   combined with (`incompatible`), the `requirements` that switch it on, and the raw `modifiers` it \
                   applies. Stances granted by a feat, enhancement or spell are on that entity's `stances` instead.",
    params(
        StancesParameters,
    ),
    responses((status = 200, description = "`total`, `limit`, `offset` and the `stances` page", body = Value),
        (status = 400, description = "Invalid sort field or malformed query parameter", body = crate::error::ErrorBody))
)]
async fn stances(State(state): State<AppState>, ApiQuery(query, _): ApiQuery) -> Result<Json<Value>, ApiError> {
    state
        .read_db(move |db| {
            let page = paged_stances_for(db, AbilityOwner::Standalone.as_str(), 0, &query, STANCES_SORT_FIELDS)?;
            Ok(Json(page.into_json("stances")))
        })
        .await
}
