mod health;
mod info;

use super::Route;

pub fn routes() -> Vec<Route> {
    [health::routes(), info::routes()].concat()
}
