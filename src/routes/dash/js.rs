use axum::routing::get;
use axum_extra::response::JavaScript;

use crate::routes::RouteType;

use super::Route;

pub const PATH: &str = "/shorten.js";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::Undocumented((PATH, get(get_shorten_js))), false)]
}

async fn get_shorten_js() -> JavaScript<&'static str> {
    JavaScript(include_str!("shorten.js"))
}
