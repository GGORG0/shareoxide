mod health;
mod user;

use utoipa_axum::{router::UtoipaMethodRouter, routes};

use crate::state::AppState;

type Route = UtoipaMethodRouter<AppState>;

pub fn routes() -> Vec<Route> {
    vec![routes!(health::health)]
}

pub fn autologin_routes() -> Vec<Route> {
    vec![routes!(user::profile)]
}
