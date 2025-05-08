mod health;
mod info;
mod link;

use super::Route;

pub fn routes() -> Vec<Route> {
    [health::routes(), info::routes(), link::routes()].concat()
}
