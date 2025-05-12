use std::ops::Deref;

use axum::{extract::State, Json};
use color_eyre::eyre::{eyre, OptionExt};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{
    axum_error::AxumResult,
    routes::RouteType,
    schema::{PartialCreated, Link, PartialLink, Shortcut, PartialShortcut},
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
        (status = OK, description = "Success", body = Vec<GetLinkResponse>)
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

/// Create a new link
#[utoipa::path(
    method(get),
    path = PATH,
    request_body = PostLinkBody,
    responses(
        (status = OK, description = "Success", body = GetLinkResponse)
    )
)]
async fn post(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
    Json(body): Json<PostLinkBody>,
) -> AxumResult<Json<GetLinkResponse>> {
    let created_link: Link = db
        .create("link")
        .content(PartialLink { url: body.url })
        .await?
        .ok_or_eyre("Failed to create link")?;

    match body.shortcuts {
        Some(shortcuts) => {
            let created_shortcuts: Vec<Shortcut> = db
                .insert("shortcut")
                .content(
                    shortcuts
                        .iter()
                        .map(|shortcut| PartialShortcut {
                            link: shortcut.clone(),
                        })
                        .collect::<Vec<_>>(),
                )
                .await?;

            // db.insert("created")
            //     .relation(created_shortcuts.iter().map(|shortcut| PartialCreated {}))
        }
        None => {}
    }

    Ok(Json(db.query(
            "SELECT id, url, <-expands_to<-shortcut AS shortcuts FROM ONLY $link WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("link", created_link.id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetLinkResponse>>(0)?.ok_or_eyre("Failed to create link")?
        ))
}

#[derive(Deserialize, Serialize, ToSchema)]
struct PostLinkBody {
    /// The short URLs to create for this link. Set to `null` to get 1 random 8-character shortcut.
    shortcuts: Option<Vec<String>>,
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
            ("id", description = "The id of the link to get")
        ),
        responses(
            (status = OK, description = "Success", body = GetLinkResponse)
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
