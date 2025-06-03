mod health;
mod info;
mod link;
mod shortcut;

use super::Route;

pub fn routes() -> Vec<Route> {
    [
        health::routes(),
        info::routes(),
        link::routes(),
        shortcut::routes(),
    ]
    .concat()
}
