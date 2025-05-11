use std::ops::Deref;

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{
    axum_error::AxumResult,
    routes::RouteType,
    serialize_recordid::{serialize_recordid_as_key, serialize_recordid_vec_as_key},
    state::SurrealDb,
    userid_extractor::SessionUserId,
};

use super::Route;

const PATH: &str = "/api/link";

pub fn routes() -> Vec<Route> {
    [
        vec![(RouteType::OpenApi(routes!(get)), true)],
        by_id::routes(),
    ]
    .concat()
}

/// Get all links you have access to
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = inline(Vec<GetLinkResponse>), content_type = "application/json")
    )
)]
async fn get(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
) -> AxumResult<Json<Vec<GetLinkResponse>>> {
    Ok(Json(
        db.query(
            "SELECT VALUE ->created->link.{id, url, shortcuts: <-expands_to<-shortcut} FROM ONLY $user",
        )
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?,
    ))
}

#[derive(Deserialize, Serialize, ToSchema)]
struct GetLinkResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_recordid_as_key")]
    id: RecordId,
    #[schema(value_type = Vec<String>)]
    #[serde(serialize_with = "serialize_recordid_vec_as_key")]
    shortcuts: Vec<RecordId>,
    url: String,
}

mod by_id {
    use axum::{extract::Path, http::StatusCode, response::IntoResponse};

    use super::*;

    const PATH: &str = "/api/link/{id}";

    pub fn routes() -> Vec<Route> {
        vec![(RouteType::OpenApi(routes!(get)), true)]
    }

    /// Get a specific link by id
    #[utoipa::path(
        method(get),
        path = PATH,
        params(
            ("id" = String, description = "The id of the link to get")
        ),
        responses(
            (status = OK, description = "Success", body = inline(GetLinkResponse), content_type = "application/json")
        )
    )]
    async fn get(
        State(db): State<SurrealDb>,
        userid: SessionUserId,
        Path(id): Path<String>,
    ) -> AxumResult<impl IntoResponse> {
        let id = RecordId::from_table_key("link", id);

        match db.query(
            "SELECT id, url, <-expands_to<-shortcut AS shortcuts FROM ONLY $link WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("link", id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetLinkResponse>>(0)? {
            Some(link) => Ok(Json(link).into_response()),
            None => Ok((StatusCode::NOT_FOUND, "Link not found").into_response()),
        }
    }
}
