mod home;
mod js;
mod styles;

use maud::{html, Markup, Render, DOCTYPE};

use super::Route;

pub fn routes() -> Vec<Route> {
    [styles::routes(), home::routes(), js::routes()].concat()
}

fn page(content: impl Render, title: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                meta charset="utf-8";

                title { "ShareOxide" @if let Some(title) = title { " - " (title) } }

                link rel="stylesheet" href=(styles::PATH);
            }

            body {
                main { (content) }

                footer {
                    a href="/apidoc/scalar" {
                        "More actions in the API"
                    }

                    a #repository href=(env!("CARGO_PKG_REPOSITORY")) {
                        (env!("CARGO_PKG_NAME")) " " (env!("CARGO_PKG_VERSION"))
                    }
                }
            }
        }
    }
}
