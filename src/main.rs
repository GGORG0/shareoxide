mod extract_url;
mod health;
mod oidc;
mod settings;
mod state;
mod user;
mod schema;

use std::{env, net::SocketAddr, sync::Arc};

use axum::{
    response::{IntoResponse, Redirect},
    Router,
};
use color_eyre::eyre::WrapErr;
use cookie::Key;
use reqwest::StatusCode;
use surrealdb::{
    engine::any::{self, Any},
    opt::auth::{Database, Namespace, Root},
    Surreal,
};
use tokio::net::TcpListener;
use tracing::{debug, info, instrument, level_filters::LevelFilter};
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

use crate::{
    oidc::init_oidc,
    settings::{env_name, Settings},
    state::{AppState, GetCookieKey as _, InnerState},
};

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    dotenvy::dotenv().ok();
    init_tracing().wrap_err("failed to set global tracing subscriber")?;

    info!(
        "Starting {} {} (built on {})...",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        env!("BUILD_TIMESTAMP")
    );

    let settings = Arc::new(Settings::try_load()?);

    let http_client = init_reqwest().wrap_err("failed to initialize HTTP client")?;
    let oidc = init_oidc(&http_client, &settings)
        .await
        .wrap_err("failed to initialize OIDC client")?;

    let db = init_surrealdb(&settings)
        .await
        .wrap_err("failed to initialize SurrealDB client")?;

    let app_state = AppState::new(InnerState {
        settings: settings.clone(),
        cookie_key: Key::get_cookie_key(),
        oidc_client: oidc,
        http_client,
        db,
    });

    let app = init_axum(app_state);
    let listener = init_listener(&settings)
        .await
        .wrap_err("failed to bind to address")?;

    info!(
        "listening on {}",
        listener
            .local_addr()
            .wrap_err("failed to get local address")?
    );

    axum::serve(listener, app.into_make_service())
        .await
        .wrap_err("failed to run server")?;

    Ok(())
}

fn init_tracing() -> color_eyre::Result<()> {
    tracing_subscriber::Registry::default()
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))
        .with(ErrorLayer::default())
        .with(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .with_env_var(env_name("LOG"))
                .from_env()?,
        )
        .try_init()?;

    Ok(())
}

fn init_reqwest() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[instrument(skip(settings))]
async fn init_surrealdb(settings: &Settings) -> Result<Surreal<Any>, surrealdb::Error> {
    let db = any::connect(&settings.db.endpoint).await?;

    debug!("Trying to sign in as a database user");
    if let Err(surrealdb::Error::Api(surrealdb::error::Api::Query(e))) = db
        .signin(Database {
            namespace: &settings.db.namespace,
            database: &settings.db.database,
            username: &settings.db.username,
            password: &settings.db.password,
        })
        .await
    {
        if e == *"There was a problem with the database: There was a problem with authentication" {
            debug!("Trying to sign in as a namespace user");
            if let Err(surrealdb::Error::Api(surrealdb::error::Api::Query(e))) = db
                .signin(Namespace {
                    namespace: &settings.db.namespace,
                    username: &settings.db.username,
                    password: &settings.db.password,
                })
                .await
            {
                if e == *"There was a problem with the database: There was a problem with authentication" {
                    debug!("Trying to sign in as a root user");
                    db.signin(Root {
                        username: &settings.db.username,
                        password: &settings.db.password,
                    })
                    .await?;
                }
            }
        }
    }

    db.use_ns(&settings.db.namespace)
        .use_db(&settings.db.database)
        .await?;

    db.query(include_str!("init.surrealql")).await?;

    Ok(db)
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

async fn init_listener(settings: &Settings) -> Result<TcpListener, std::io::Error> {
    let addr: Vec<SocketAddr> = settings.general.listen_address.clone().into();

    TcpListener::bind(addr.as_slice()).await
}
