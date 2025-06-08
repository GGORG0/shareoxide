use std::ops::Deref as _;

use axum::{extract::State, response::Redirect, routing::get};
use axum_oidc::OidcClaims;
use maud::{html, Markup};

use crate::{
    axum_error::AxumResult, routes::{api::link::GetLinkResponse, dash::page, RouteType}, state::SurrealDb, userid_extractor::SessionUserId, GroupClaims
};

use super::Route;

pub const PATH: &str = "/dash";

pub fn routes() -> Vec<Route> {
    vec![(RouteType::Undocumented((PATH, get(get_dash_home))), true), 
         (RouteType::Undocumented(("/", get(index_dash_redirect))), false)]
}

async fn get_dash_home(
    State(db): State<SurrealDb>,
    userid: SessionUserId, claims: OidcClaims<GroupClaims>) -> AxumResult<Markup> {
    let user_name = claims.name().and_then(|x| x.get(None)).map(|x|x.deref().clone()).unwrap_or("there".to_string());

    let links: Vec<GetLinkResponse> = 
        db.query(
            "SELECT VALUE ->created->link.{id, url, shortcuts: <-expands_to<-shortcut.shortlink} FROM ONLY $user",
        )
        .bind(("user", userid.deref().clone()))
        .await?
        .take(0)?;

    Ok(page(
        html! {
            h1 { "Hi " (user_name) "!" }

            form id="shorten-form" {
                input type="text" id="link-input" name="link" placeholder="Enter a link to shorten" required;
                input type="text" id="shortlink-input" name="shortlink" placeholder="Custom short link";
                button type="submit" { "Shorten!" }
            }

            script src="/shorten.js" {}

            h2 { "Your Links" }

            table id="links" {
                thead {
                    tr {
                        th { "URL" }
                        th { "Short link" }
                    }
                }
                tbody {
                    @for link in links {
                        tr {
                            td { a href=(link.url) { (link.url) } }
                            td { pre { (link.shortcuts.join(", ")) } }
                        }
                    }
                }
            }
        },
        None,
    ))
}

async fn index_dash_redirect() -> Redirect {
    Redirect::to("/dash")
}