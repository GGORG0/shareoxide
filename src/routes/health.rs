/// Get health of the service (returns "ok")
#[utoipa::path(
    method(get),
    path = "/health",
    responses(
        (status = OK, description = "Success", body = str, content_type = "text/plain")
    )
)]
pub async fn health() -> &'static str {
    "ok"
}
