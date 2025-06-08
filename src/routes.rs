mod api;
mod dash;
mod shortcut_handler;

use axum::routing::MethodRouter;
use utoipa_axum::router::UtoipaMethodRouter;

use crate::state::AppState;

pub fn routes() -> Vec<Route> {
    [shortcut_handler::routes(), api::routes(), dash::routes()].concat()
}

type Route = (RouteType, bool);

#[derive(Clone)]
pub enum RouteType {
    OpenApi(UtoipaMethodRouter<AppState>),
    Undocumented((&'static str, MethodRouter<AppState>)),
}
