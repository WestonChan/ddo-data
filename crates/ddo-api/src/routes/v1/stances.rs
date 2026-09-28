use crate::db::stances_for;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use ddo_model::enums::AbilityOwner;
use serde_json::Value;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(list))
}

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
    responses((status = 200, description = "Every standalone stance", body = Vec<Value>))
)]
async fn list(State(state): State<AppState>) -> Result<Json<Vec<Value>>, ApiError> {
    state.query(|conn| Ok(Json(stances_for(conn, AbilityOwner::Standalone.as_str(), 0)?))).await
}
