mod health;
mod schema;
mod settings;
mod state;
mod user;

use std::{net::SocketAddr, ops::Deref, sync::Arc};

use axum::{
    error_handling::HandleErrorLayer, http::StatusCode, response::IntoResponse, routing::any,
    Router,
};
use axum_oidc::{
    error::MiddlewareError, handle_oidc_redirect, OidcAuthLayer, OidcClient, OidcLoginLayer,
};
use color_eyre::eyre::WrapErr;
use color_eyre::Result;
use serde::{Deserialize, Serialize};
use surrealdb::{
    engine::any::{self, Any},
    opt::auth::{Database, Namespace, Root},
    Surreal,
};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};
use tower_sessions_file_store::FileSessionStorage;
use tracing::{debug, error, info, info_span, instrument, level_filters::LevelFilter, Instrument};
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
    settings::{env_name, Settings},
    state::{AppState, InnerState},
};

#[derive(OpenApi)]
#[openapi()]
struct ApiDoc;

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct GroupClaims {
    groups: Vec<String>,
}
impl axum_oidc::AdditionalClaims for GroupClaims {}
impl openidconnect::AdditionalClaims for GroupClaims {}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    dotenvy::dotenv().ok();
    init_tracing().wrap_err("failed to set global tracing subscriber")?;

    info!(
        "Starting {} {}...",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
    );

    let settings = Arc::new(Settings::try_load()?);

    let db = init_surrealdb(&settings).await?;

    let app_state = AppState::new(InnerState {
        settings: settings.clone(),
        db,
    });

    let app = init_axum(app_state).await?;
    let listener = init_listener(&settings).await?;

    info!(
        "listening on {} ({})",
        listener
            .local_addr()
            .wrap_err("failed to get local address")?,
        settings.general.public_url
    );

    axum::serve(listener, app.into_make_service())
        .await
        .wrap_err("failed to run server")?;

    Ok(())
}

fn init_tracing() -> Result<()> {
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

#[instrument(skip(settings))]
async fn init_surrealdb(settings: &Settings) -> Result<Surreal<Any>> {
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

#[instrument(skip(state))]
async fn init_axum(state: AppState) -> Result<Router> {
    let session_store = FileSessionStorage::new();

    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_same_site(SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(
            tower_sessions::cookie::time::Duration::seconds(60 * 60),
        ));

    let handle_error_layer = HandleErrorLayer::new(|e: MiddlewareError| async {
        error!(error = ?e, "An error occurred in OIDC middleware");
        e.into_response()
    });

    let oidc_login_service = ServiceBuilder::new()
        .layer(handle_error_layer.clone())
        .layer(OidcLoginLayer::<GroupClaims>::new());

    let mut oidc_client = OidcClient::<GroupClaims>::builder()
        .with_default_http_client()
        .with_redirect_url(format!("{}/oidc", state.settings.general.public_url).parse()?)
        .with_client_id(state.settings.oidc.client_id.as_str())
        .add_scope("profile")
        .add_scope("offline_access");

    if let Some(client_secret) = state.settings.oidc.client_secret.as_ref() {
        oidc_client = oidc_client.with_client_secret(client_secret.secret().clone());
    }

    let oidc_client = oidc_client
        .discover(state.settings.oidc.issuer.deref().clone())
        .instrument(info_span!("OIDC discovery"))
        .await?
        .build();

    let oidc_auth_service = ServiceBuilder::new()
        .layer(handle_error_layer)
        .layer(OidcAuthLayer::new(oidc_client));

    let autologin_router = OpenApiRouter::new()
        .routes(routes!(user::profile))
        .layer(oidc_login_service);

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(autologin_router)
        .routes(routes!(health::health))
        .route("/oidc", any(handle_oidc_redirect::<GroupClaims>))
        .with_state(state)
        .split_for_parts();

    let openapi_prefix = "/apidoc";
    let spec_path = format!("{openapi_prefix}/openapi.json");

    let router = router
        .merge(
            SwaggerUi::new(format!("{openapi_prefix}/swagger-ui"))
                .url(spec_path.clone(), api.clone()),
        )
        .merge(Redoc::with_url(
            format!("{openapi_prefix}/redoc"),
            api.clone(),
        ))
        .merge(RapiDoc::new(spec_path).path(format!("{openapi_prefix}/rapidoc")))
        .merge(Scalar::with_url(format!("{openapi_prefix}/scalar"), api));

    let router = router
        .layer(oidc_auth_service)
        .layer(session_layer)
        .fallback(|| async { (StatusCode::NOT_FOUND, "Not found").into_response() });

    Ok(router)
}

async fn init_listener(settings: &Settings) -> Result<TcpListener> {
    let addr: Vec<SocketAddr> = settings.general.listen_address.clone().into();

    Ok(TcpListener::bind(addr.as_slice()).await?)
}
