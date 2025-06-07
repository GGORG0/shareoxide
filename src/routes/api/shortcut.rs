use std::ops::Deref;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use color_eyre::eyre::{eyre, ContextCompat, OptionExt};
use rand::distr::{Alphanumeric, SampleString as _};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{
    axum_error::AxumResult,
    routes::RouteType,
    schema::{Created, ExpandsTo, PartialCreated, PartialExpandsTo, PartialShortcut, Shortcut},
    serialize_recordid::{deserialize_recordid_from_key_for_link, serialize_recordid_as_key},
    state::SurrealDb,
    userid_extractor::SessionUserId,
};

use super::Route;

const PATH: &str = "/api/shortcut";

pub fn routes() -> Vec<Route> {
    [
        vec![(
            RouteType::OpenApi(routes!(get_shortcut_list, post_shortcut_list)),
            true,
        )],
        by_id::routes(),
    ]
    .concat()
}

#[derive(Deserialize, Serialize, ToSchema)]
struct GetShortcutResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_recordid_as_key")]
    id: RecordId,
    shortlink: String,
    // TODO: add the expanded link
}

/// Get all shortcuts you have access to
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = Vec<GetShortcutResponse>)
    )
)]
async fn get_shortcut_list(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
) -> AxumResult<Json<Vec<GetShortcutResponse>>> {
    Ok(Json(
        db.query("SELECT VALUE ->created->shortcut.{id, shortlink} FROM ONLY $user")
            .bind(("user", userid.deref().clone()))
            .await?
            .take(0)?,
    ))
}

/// Create a new shortcut
#[utoipa::path(
    method(post),
    path = PATH,
    request_body = PostShortcutBody,
    responses(
        (status = OK, description = "Success", body = GetShortcutResponse)
    )
)]
async fn post_shortcut_list(
    State(db): State<SurrealDb>,
    userid: SessionUserId,
    Json(body): Json<PostShortcutBody>,
) -> AxumResult<impl IntoResponse> {
    let shortlink = body.shorturl.unwrap_or_else(|| {
        let mut rng = rand::rng();
        Alphanumeric.sample_string(&mut rng, 10)
    });

    let collision: Vec<String> = db
        .query("SELECT VALUE shortlink FROM shortcut WHERE shortlink = $shortlink")
        .bind(("shortlink", shortlink.clone()))
        .await?
        .take(0)?;

    if !collision.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            format!("Shortcut already exists: {}", collision.join(", ")),
        )
            .into_response());
    }

    let link_id = db
        .query(
            "SELECT VALUE id FROM link WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("link", body.link))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<RecordId>>(0)?
        .ok_or_eyre("Link not found")?;

    let created_shortcut: Shortcut = db
        .create("shortcut")
        .content(PartialShortcut { shortlink })
        .await?
        .wrap_err("Failed to create shortcut")?;

    if (db
        .insert("created")
        .relation(PartialCreated {
            object: created_shortcut.id.clone(),
            user: userid.deref().clone(),
        })
        .await? as Vec<Created>)
        .is_empty()
    {
        return Err(eyre!("Failed to create shortcut").into());
    }

    if (db
        .insert("expands_to")
        .relation(PartialExpandsTo {
            object: link_id.clone(),
            shortcut: created_shortcut.id.clone(),
        })
        .await? as Vec<ExpandsTo>)
        .is_empty()
    {
        return Err(eyre!("Failed to create shortcut").into());
    }

    Ok(Json(db.query(
            "SELECT id, shortlink FROM ONLY $shortcut WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("shortcut", created_shortcut.id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetShortcutResponse>>(0)?.ok_or_eyre("Failed to create shortcut")?
    ).into_response())
}

#[derive(Deserialize, Serialize, ToSchema)]
struct PostShortcutBody {
    /// The short URL to create for the specified link. Set to `null` to get 1 random 10-character shortcut.
    shorturl: Option<String>,

    /// The ID of the link to create the shortcut for.
    #[schema(value_type = String)]
    #[serde(deserialize_with = "deserialize_recordid_from_key_for_link")]
    link: RecordId,
}

mod by_id {
    use axum::{extract::Path, http::StatusCode, response::IntoResponse};

    use super::*;

    const PATH: &str = "/api/shortcut/{id}";

    pub fn routes() -> Vec<Route> {
        vec![(
            RouteType::OpenApi(routes!(get_shortcut, delete_shortcut)),
            true,
        )]
    }

    /// Get a specific shortcut by id
    #[utoipa::path(
        method(get),
        path = PATH,
        params(
            ("id", description = "The id of the shortcut to get")
        ),
        responses(
            (status = OK, description = "Success", body = GetShortcutResponse)
        )
    )]
    async fn get_shortcut(
        State(db): State<SurrealDb>,
        userid: SessionUserId,
        Path(id): Path<String>,
    ) -> AxumResult<impl IntoResponse> {
        let id = RecordId::from_table_key("shortcut", id);

        match db.query(
            "SELECT id, shortlink FROM ONLY $shortcut WHERE array::any(array::matches(<-created<-user.id, $user))",
        )
        .bind(("shortcut", id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take::<Option<GetShortcutResponse>>(0)? {
            Some(link) => Ok(Json(link).into_response()),
            None => Ok((StatusCode::NOT_FOUND, "Shortcut not found").into_response()),
        }
    }

    /// Delete a shortcut
    #[utoipa::path(
        method(delete),
        path = PATH,
        params(
            ("id", description = "The id of the shortcut to delete")
        ),
        responses(
            (status = OK, description = "Success", body = GetShortcutResponse)
        )
    )]
    async fn delete_shortcut(
        State(db): State<SurrealDb>,
        userid: SessionUserId,
        Path(id): Path<String>,
    ) -> AxumResult<impl IntoResponse> {
        let id = RecordId::from_table_key("shortcut", id);

        let deleted: Option<bool> = db.query(
            "
                BEGIN;
                IF array::len(SELECT id FROM $shortcut WHERE array::any(array::matches(<-created<-user.id, $user))) == 0 {
                    RETURN FALSE;
                    CANCEL;
                } ELSE {
                    TRUE
                };
                DELETE ONLY $shortcut<-created RETURN BEFORE;
                DELETE ONLY $shortcut->expands_to RETURN BEFORE;
                DELETE ONLY $shortcut RETURN BEFORE;
                COMMIT;
            ",
        )
        .bind(("shortcut", id))
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?;

        Ok(if matches!(deleted, Some(false) | None) {
            (StatusCode::NOT_FOUND, "Shortcut not found").into_response()
        } else {
            ("ShortcuShortcut deleted successfully").into_response()
        })
    }
}
