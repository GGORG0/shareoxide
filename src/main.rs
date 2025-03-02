mod extract_url;
mod health;
mod oidc;
mod state;
mod user;

use std::{env, error::Error};

use axum::{
    response::{IntoResponse, Redirect},
    Router,
};
use cookie::Key;
use reqwest::StatusCode;
use state::{AppState, GetCookieKey as _, InnerState};
use tokio::net::TcpListener;
use tracing::{info, level_filters::LevelFilter};
use tracing_error::ErrorLayer;
use tracing_subscriber::{
    fmt::format::FmtSpan, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_scalar::{Scalar, Servable as _};
use utoipa_swagger_ui::SwaggerUi;

use crate::oidc::init_oidc;

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    init_tracing().expect("failed to set global tracing subscriber");

    info!("Starting...");

    let http_client = init_reqwest();
    let oidc = init_oidc(&http_client)
        .await
        .expect("failed to initialize OIDC client");

    let app_state = AppState::new(InnerState {
        key: Key::get_cookie_key(),
        oidc_client: oidc,
        http_client,
    });

    let app = init_axum(app_state);
    let listener = init_listener().await.expect("failed to bind to address");

    info!(
        "listening on {}",
        listener.local_addr().expect("failed to get local address")
    );

    axum::serve(listener, app.into_make_service())
        .await
        .expect("failed to run server");
}

fn init_tracing() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::Registry::default()
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
        .with(ErrorLayer::default())
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .try_init()?;

    Ok(())
}

fn init_reqwest() -> reqwest::Client {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Client should build")
}

fn init_axum(state: AppState) -> Router {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(health::health))
        .nest("/auth", oidc::api::router())
        .nest("/user", user::router(state.clone()))
        .with_state(state)
        .split_for_parts();

    let spec_path = "/apidoc/openapi.json";

    let router = router
        .merge(SwaggerUi::new("/swagger-ui").url(spec_path, api.clone()))
        .merge(Redoc::with_url("/redoc", api.clone()))
        .merge(RapiDoc::new(spec_path).path("/rapidoc"))
        .merge(Scalar::with_url("/scalar", api));

    router.merge(
        Router::new()
            .route(
                "/",
                axum::routing::get(|| async { Redirect::temporary("/scalar") }),
            )
            .fallback(|| async { (StatusCode::NOT_FOUND, "Not found").into_response() }),
    )
}

async fn init_listener() -> Result<TcpListener, std::io::Error> {
    TcpListener::bind(env::var("SERVER_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8080".to_string()))
        .await
}
