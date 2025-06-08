use axum::routing::get;
use axum_extra::response::Css;

use crate::routes::RouteType;

use super::Route;

pub const PATH: &str = "/styles.css";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::Undocumented((PATH, get(get_styles))), false)]
}

async fn get_styles() -> Css<&'static str> {
    Css(include_str!("styles.css"))
}
