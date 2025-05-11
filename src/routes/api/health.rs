use utoipa_axum::routes;

use crate::routes::RouteType;

use super::Route;

const PATH: &str = "/api/health";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::OpenApi(routes!(get)), false)]
}

/// Get health of the service (returns "ok")
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = str)
    )
)]
async fn get() -> &'static str {
    "ok"
}
