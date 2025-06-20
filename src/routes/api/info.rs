use axum::{extract::State, Json};
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::routes;

use crate::{routes::RouteType, settings::ArcSettings};

use super::Route;

const PATH: &str = "/api/info";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::OpenApi(routes!(get_info)), false)]
}

/// Get information about the service
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = GetInfoResponse)
    )
)]
async fn get_info(State(settings): State<ArcSettings>) -> Json<GetInfoResponse> {
    Json(GetInfoResponse {
        name: env!("CARGO_PKG_NAME"),
        version: env!("CARGO_PKG_VERSION"),
        repo: env!("CARGO_PKG_REPOSITORY"),
        public_url: settings.general.public_url.to_string(),
    })
}

#[derive(Serialize, ToSchema)]
struct GetInfoResponse {
    name: &'static str,
    version: &'static str,
    repo: &'static str,
    public_url: String,
}
