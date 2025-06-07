use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use utoipa_axum::routes;

use crate::{axum_error::AxumResult, routes::RouteType, state::SurrealDb};

use super::Route;

const PATH: &str = "/{shortlink}";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::OpenApi(routes!(get_shortcut_redirect)), false)]
}

/// Redirects you to the destination of the shortcut
#[utoipa::path(
    method(get),
    path = PATH,
    params(
        ("shortlink" = String, Path, description = "The short link to redirect to")
    ),
    responses(
        (status = OK, description = "Success", body = str)
    )
)]
async fn get_shortcut_redirect(
    State(db): State<SurrealDb>,
    Path(shortlink): Path<String>,
) -> AxumResult<impl IntoResponse> {
    match db
        .query(
            "SELECT VALUE ->expands_to->link.url FROM ONLY shortcut WHERE shortlink = $shortlink",
        )
        .bind(("shortlink", shortlink))
        .await?
        .take::<Option<String>>(0)?
    {
        Some(url) => Ok(Redirect::temporary(url.as_str()).into_response()),
        None => Ok((StatusCode::NOT_FOUND, "Shortcut not found").into_response()),
    }
}
