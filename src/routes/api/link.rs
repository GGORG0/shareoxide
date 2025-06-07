use std::ops::Deref;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use color_eyre::eyre::{eyre, OptionExt};
use rand::distr::{Alphanumeric, SampleString as _};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{
    axum_error::AxumResult,
    routes::RouteType,
    schema::{
        Created, ExpandsTo, Link, PartialCreated, PartialExpandsTo, PartialLink, PartialShortcut,
        Shortcut,
    },
    serialize_recordid::serialize_recordid_as_key,
    state::SurrealDb,
    userid_extractor::SessionUserId,
};

use super::Route;

const PATH: &str = "/api/link";

pub fn routes() -> Vec<Route> {
    [
        vec![(
            RouteType::OpenApi(routes!(get_link_list, post_link_list)),
            true,
        )],
        by_id::routes(),
    ]
    .concat()
}

#[derive(Deserialize, Serialize, ToSchema)]
struct GetLinkResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_recordid_as_key")]
    id: RecordId,
    shortcuts: Vec<String>,
    url: String,
}

/// Get all links you have access to
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = Vec<GetLinkResponse>)
    )
)]
async fn get_link_list(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
) -> AxumResult<Json<Vec<GetLinkResponse>>> {
    Ok(Json(
        db.query(
            "SELECT VALUE ->created->link.{id, url, shortcuts: <-expands_to<-shortcut.shortlink} FROM ONLY $user",
        )
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?,
    ))
}

/// Create a new link
#[utoipa::path(
    method(post),
    path = PATH,
    request_body = PostLinkBody,
    responses(
        (status = OK, description = "Success", body = GetLinkResponse)
    )
)]
async fn post_link_list(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
    Json(body): Json<PostLinkBody>,
) -> AxumResult<impl IntoResponse> {
    let shortcuts = body.shortcuts.unwrap_or_else(|| {
        let mut rng = rand::rng();
        let shortcut = Alphanumeric.sample_string(&mut rng, 10);
        vec![shortcut]
    });

    let collisions: Vec<String> = db
        .query("SELECT VALUE shortlink FROM shortcut WHERE array::any(array::matches($shortcuts, shortlink))")
        .bind(("shortcuts", shortcuts.clone()))
        .await?
        .take(0)?;

    if !collisions.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            format!("Shortcuts already exist: {}", collisions.join(", ")),
        )
            .into_response());
    }

    let created_link: Link = db
        .create("link")
        .content(PartialLink { url: body.url })
        .await?
        .ok_or_eyre("Failed to create link")?;

    let link_created_rel: Vec<Created> = db
        .insert("created")
        .relation(PartialCreated {
            object: created_link.id.clone(),
            user: userid.deref().clone(),
        })
        .await?;

    if link_created_rel.is_empty() {
        let _: Option<Link> = db.delete(&created_link.id).await?;
        return Err(eyre!("Failed to create link").into());
    }

    let created_shortcuts: Vec<Shortcut> = db
        .insert("shortcut")
        .content(
            shortcuts
                .iter()
                .map(|shortcut| PartialShortcut {
                    shortlink: shortcut.clone(),
                })
                .collect::<Vec<_>>(),
        )
        .await?;

    if created_shortcuts.len() != created_shortcuts.len() {
        return Err(eyre!("Failed to create shortcuts").into());
    }

    let shortcuts_created_rel: Vec<Created> = db
        .insert("created")
        .relation(
            created_shortcuts
                .iter()
                .map(|shortcut| PartialCreated {
                    object: shortcut.id.clone(),
                    user: userid.deref().clone(),
                })
                .collect::<Vec<_>>(),
        )
        .await?;

    if shortcuts_created_rel.len() != created_shortcuts.len() {
        return Err(eyre!("Failed to create shortcuts").into());
    }

    let expands_to_rel: Vec<ExpandsTo> = db
        .insert("expands_to")
        .relation(
            created_shortcuts
                .iter()
                .map(|shortcut| PartialExpandsTo {
                    object: created_link.id.clone(),
                    shortcut: shortcut.id.clone(),
                })
                .collect::<Vec<_>>(),
        )
        .await?;

    if expands_to_rel.len() != created_shortcuts.len() {
        return Err(eyre!("Failed to create shortcuts").into());
    }

    Ok(Json(db.query(
            "SELECT id, url, <-expands_to<-shortcut.shortlink AS shortcuts FROM ONLY $link WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("link", created_link.id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetLinkResponse>>(0)?.ok_or_eyre("Failed to create link")?
        ).into_response())
}

#[derive(Deserialize, Serialize, ToSchema)]
struct PostLinkBody {
    /// The short URLs to create for this link. Set to `null` to get 1 random 10-character shortcut.
    shortcuts: Option<Vec<String>>,
    url: String,
}

mod by_id {
    use axum::{extract::Path, http::StatusCode, response::IntoResponse};

    use super::*;

    const PATH: &str = "/api/link/{id}";

    pub fn routes() -> Vec<Route> {
        vec![(RouteType::OpenApi(routes!(get_link, delete_link)), true)]
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
    async fn get_link(
        State(db): State<SurrealDb>,
        userid: SessionUserId,
        Path(id): Path<String>,
    ) -> AxumResult<impl IntoResponse> {
        let id = RecordId::from_table_key("link", id);

        match db.query(
            "SELECT id, url, <-expands_to<-shortcut.shortlink AS shortcuts FROM ONLY $link WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("link", id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetLinkResponse>>(0)? {
            Some(link) => Ok(Json(link).into_response()),
            None => Ok((StatusCode::NOT_FOUND, "Link not found").into_response()),
        }
    }

    /// Delete a link and all shortcuts pointing to it
    #[utoipa::path(
        method(delete),
        path = PATH,
        params(
            ("id", description = "The id of the link to delete")
        ),
        responses(
            (status = OK, description = "Success", body = str)
        )
    )]
    async fn delete_link(
        State(db): State<SurrealDb>,
        userid: SessionUserId,
        Path(id): Path<String>,
    ) -> AxumResult<impl IntoResponse> {
        let id = RecordId::from_table_key("link", id);

        let deleted: Option<bool> = db.query(
            "
                BEGIN;
                IF array::len(SELECT id FROM $link WHERE array::any(array::matches(<-created<-user.id, $user))) == 0 {
                    RETURN FALSE;
                    CANCEL;
                } ELSE {
                    TRUE
                };
                DELETE ONLY $link<-created RETURN BEFORE;
                DELETE (SELECT VALUE array::flatten([<-expands_to, <-expands_to<-shortcut, <-expands_to<-shortcut<-created]) FROM ONLY $link) RETURN BEFORE;
                DELETE ONLY $link RETURN BEFORE;
                COMMIT;
            ",
        )
        .bind(("link", id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?;

        Ok(if matches!(deleted, Some(false) | None) {
            (StatusCode::NOT_FOUND, "Link not found").into_response()
        } else {
            ("Link deleted successfully").into_response()
        })
    }
}
