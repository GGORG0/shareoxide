// Temporary endpoints for OpenID Connect testing

use axum::{middleware, Extension, Json};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    oidc::{auth_middleware, GroupIdTokenClaims},
    state::AppState,
};

pub fn router(state: AppState) -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(profile))
        .layer(middleware::from_fn_with_state(state, auth_middleware))
}

/// Get the current user's profile claims
#[utoipa::path(
    method(get),
    path = "/profile",
    responses(
        (status = OK, description = "Success", body = serde_json::Value, content_type = "application/json")
    )
)]
async fn profile(Extension(claims): Extension<GroupIdTokenClaims>) -> Json<GroupIdTokenClaims> {
    Json(claims)
}
