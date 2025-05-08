use std::ops::Deref;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{
    axum_error::AxumResult, routes::RouteType, state::SurrealDb, userid_extractor::SessionUserId,
};

use super::Route;

const PATH: &str = "/api/link";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::OpenApi(routes!(get)), false)]
}

/// Get all links you have access to
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = inline(Vec<GetLinksResponse>), content_type = "application/json")
    )
)]
async fn get(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
) -> AxumResult<Json<Vec<GetLinksResponse>>> {
    Ok(Json(
        db.query(
            "SELECT VALUE ->created->link.{id, url, shortcuts: <-expands_to<-shortcut} FROM $user",
        )
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?,
    ))
}

#[derive(Deserialize, Serialize, ToSchema)]
struct GetLinksResponse {
    #[schema(value_type = String)]
    id: RecordId,
    #[schema(value_type = Vec<String>)]
    shortcuts: Vec<RecordId>,
    url: String,
}
