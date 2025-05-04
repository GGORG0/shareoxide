// Temporary endpoints for OpenID Connect testing

use axum::Json;
use axum_oidc::OidcClaims;
use openidconnect::{core::CoreGenderClaim, IdTokenClaims};

use crate::GroupClaims;

/// Get the current user's profile claims
#[utoipa::path(
    method(get),
    path = "/user/profile",
    responses(
        (status = OK, description = "Success", body = serde_json::Value, content_type = "application/json")
    )
)]
pub async fn profile(
    claims: OidcClaims<GroupClaims>,
) -> Json<IdTokenClaims<GroupClaims, CoreGenderClaim>> {
    Json(claims.0)
}
