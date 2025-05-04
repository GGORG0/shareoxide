mod api;

use utoipa_axum::router::UtoipaMethodRouter;

use crate::state::AppState;

type Route = (UtoipaMethodRouter<AppState>, bool);

pub fn routes() -> Vec<Route> {
    [api::routes()].concat()
}
