use utoipa_axum::routes;

use super::Route;

const PATH: &str = "/api/health";

pub fn routes() -> Vec<Route> {
    vec![(routes!(get), false)]
}

/// Get health of the service (returns "ok")
#[utoipa::path(
    method(get),
    path = PATH,
    responses(
        (status = OK, description = "Success", body = str, content_type = "text/plain")
    )
)]
async fn get() -> &'static str {
    "ok"
}
