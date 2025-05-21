use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use utoipa_axum::routes;

use crate::{axum_error::AxumResult, routes::RouteType, state::SurrealDb};

use super::Route;

const PATH: &str = "/{shortcut}";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::OpenApi(routes!(get)), false)]
}

/// Redirects you to the destination of the shortcut
#[utoipa::path(
    method(get),
    path = PATH,
    params(
        ("shortcut" = String, Path, description = "The shortcut link to redirect to")
    ),
    responses(
        (status = OK, description = "Success", body = str)
    )
)]
async fn get(State(db): State<SurrealDb>, Path(id): Path<String>) -> AxumResult<impl IntoResponse> {
    match db
        .query("SELECT VALUE ->expands_to->link.url FROM ONLY shortcut WHERE link = $shortcut")
        .bind(("shortcut", id))
        .await?
        .take::<Option<String>>(0)?
    {
        Some(url) => Ok(Redirect::temporary(url.as_str()).into_response()),
        None => Ok((StatusCode::NOT_FOUND, "Shortcut not found").into_response()),
    }
}
