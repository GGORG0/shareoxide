use std::str::FromStr as _;

use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::PrivateCookieJar;
use cookie::{time::Duration, Cookie};
use openidconnect::{
    core::{self, CoreGenderClaim, CoreIdToken, CoreIdTokenClaims, CoreTokenResponse},
    AccessToken, AdditionalClaims, AdditionalProviderMetadata, ClaimsVerificationError,
    EndpointMaybeSet, EndpointNotSet, EndpointSet, HttpClientError, IdTokenClaims, Nonce,
    OAuth2TokenResponse as _, ProviderMetadata, RefreshToken, RevocationUrl, TokenResponse,
    UserInfoClaims, UserInfoError,
};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument};

use crate::{extract_url::ExtractUrl, settings::Settings, state::AppState};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RevocationEndpointProviderMetadata {
    revocation_endpoint: String,
}
impl AdditionalProviderMetadata for RevocationEndpointProviderMetadata {}
type RevocableProviderMetadata = ProviderMetadata<
    RevocationEndpointProviderMetadata,
    core::CoreAuthDisplay,
    core::CoreClientAuthMethod,
    core::CoreClaimName,
    core::CoreClaimType,
    core::CoreGrantType,
    core::CoreJweContentEncryptionAlgorithm,
    core::CoreJweKeyManagementAlgorithm,
    core::CoreJsonWebKey,
    core::CoreResponseMode,
    core::CoreResponseType,
    core::CoreSubjectIdentifierType,
>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GroupClaims {
    groups: Vec<String>,
}
impl AdditionalClaims for GroupClaims {}

pub type GroupIdTokenClaims = IdTokenClaims<GroupClaims, CoreGenderClaim>;

pub type OidcClient = core::CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Error, Debug)]
pub enum InitOidcError {
    #[error("reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] openidconnect::url::ParseError),

    #[error("OIDC discovery error: {0}")]
    Discovery(
        #[from] openidconnect::DiscoveryError<openidconnect::HttpClientError<reqwest::Error>>,
    ),
}

#[instrument(skip(http_client, settings))]
pub async fn init_oidc(
    http_client: &reqwest::Client,
    settings: &Settings,
) -> Result<OidcClient, InitOidcError> {
    // TODO: handle providers without a revocation endpoint
    let provider_metadata =
        RevocableProviderMetadata::discover_async(settings.oidc.issuer.clone(), http_client)
            .await?;

    let revocation_endpoint = provider_metadata
        .additional_metadata()
        .revocation_endpoint
        .clone();

    let client = core::CoreClient::from_provider_metadata(
        provider_metadata,
        settings.oidc.client_id.clone(),
        Some(settings.oidc.client_secret.clone()),
    )
    .set_revocation_url(RevocationUrl::new(revocation_endpoint)?);

    Ok(client)
}

#[derive(Debug)]
struct AuthCookie {
    id_token: CoreIdToken,
    access_token: AccessToken,
    refresh_token: RefreshToken,
    additional_claims: GroupClaims,
}

impl AuthCookie {
    fn add_to_jar(&self, mut jar: PrivateCookieJar) -> PrivateCookieJar {
        let cookies = [
            ("id_token", self.id_token.to_string()),
            ("access_token", self.access_token.secret().clone()),
            ("refresh_token", self.refresh_token.secret().clone()),
            (
                "additional_claims",
                serde_json::to_string(&self.additional_claims).unwrap(),
            ),
        ];

        for (name, value) in cookies {
            jar = jar.add(
                Cookie::build((name, value))
                    .path("/")
                    .max_age(Duration::days(14)),
            );
        }

        jar
    }

    fn get_from_jar(jar: &PrivateCookieJar) -> Result<Self, String> {
        let id_token = match jar.get("id_token") {
            Some(cookie) => {
                CoreIdToken::from_str(cookie.value_trimmed()).map_err(|err| err.to_string())?
            }
            None => return Err("ID token missing".to_string()),
        };

        let access_token = match jar.get("access_token") {
            Some(cookie) => AccessToken::new(cookie.value_trimmed().to_string()),
            None => return Err("Access token missing".to_string()),
        };

        let refresh_token = match jar.get("refresh_token") {
            Some(cookie) => RefreshToken::new(cookie.value_trimmed().to_string()),
            None => return Err("Refresh token missing".to_string()),
        };

        let additional_claims = match jar.get("additional_claims") {
            Some(cookie) => {
                serde_json::from_str(cookie.value_trimmed()).map_err(|err| err.to_string())?
            }
            None => return Err("Additional claims missing".to_string()),
        };

        Ok(Self {
            id_token,
            access_token,
            refresh_token,
            additional_claims,
        })
    }
}

#[derive(Error, Debug)]
enum ProcessTokenResponseError {
    #[error("missing refresh token")]
    MissingRefreshToken,

    #[error("missing nonce")]
    MissingNonce,

    #[error("missing ID token")]
    MissingIdToken,

    #[error("ID token error: {0}")]
    IdTokenError(#[from] ClaimsVerificationError),

    #[error("user info error: {0}")]
    UserInfoError(#[from] UserInfoError<HttpClientError<reqwest::Error>>),
}

async fn process_token_response(
    oidc_client: &OidcClient,
    http_client: &reqwest::Client,
    jar: &PrivateCookieJar,
    token_response: CoreTokenResponse,
) -> Result<(AuthCookie, CoreIdTokenClaims), ProcessTokenResponseError> {
    let access_token = token_response.access_token();
    let refresh_token = token_response
        .refresh_token()
        .ok_or(ProcessTokenResponseError::MissingRefreshToken)?;
    let nonce = jar
        .get("nonce")
        .map(|cookie| Nonce::new(cookie.value_trimmed().to_string()))
        .ok_or(ProcessTokenResponseError::MissingNonce)?;
    let id_token = token_response
        .id_token()
        .ok_or(ProcessTokenResponseError::MissingIdToken)?;

    let id_token_claims: CoreIdTokenClaims = id_token
        .claims(&oidc_client.id_token_verifier(), &nonce)?
        .clone();

    let userinfo_claims: UserInfoClaims<GroupClaims, CoreGenderClaim> = oidc_client
        .user_info(
            token_response.access_token().to_owned(),
            Some(id_token_claims.subject().to_owned()),
        )
        .expect("no user info endpoint")
        .request_async(http_client)
        .await?;

    Ok((
        AuthCookie {
            id_token: id_token.clone(),
            access_token: access_token.clone(),
            refresh_token: refresh_token.clone(),
            additional_claims: userinfo_claims.additional_claims().clone(),
        },
        id_token_claims,
    ))
}

fn merge_claims(
    id_token_claims: &CoreIdTokenClaims,
    group_claims: &GroupClaims,
) -> Result<GroupIdTokenClaims, serde_json::Error> {
    // very hacky!

    #[derive(Debug, Serialize, Deserialize)]
    struct OptionalGroupClaims {
        groups: Option<Vec<String>>,
    }
    impl From<GroupClaims> for OptionalGroupClaims {
        fn from(group_claims: GroupClaims) -> Self {
            Self {
                groups: Some(group_claims.groups),
            }
        }
    }
    impl AdditionalClaims for OptionalGroupClaims {}

    let id_token_claims_json = serde_json::to_value(id_token_claims)?;

    let mut optional_group_id_token_claims: IdTokenClaims<OptionalGroupClaims, CoreGenderClaim> =
        serde_json::from_value(id_token_claims_json)?;

    *optional_group_id_token_claims.additional_claims_mut() = group_claims.clone().into();

    let optional_group_id_token_claims_json =
        serde_json::to_value(&optional_group_id_token_claims)?;

    let group_id_token_claims: GroupIdTokenClaims =
        serde_json::from_value(optional_group_id_token_claims_json)?;

    Ok(group_id_token_claims)
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    ExtractUrl(this_url): ExtractUrl,
    mut jar: PrivateCookieJar,
    mut request: Request,
    next: Next,
) -> Response {
    let re_authenticate = {
        let this_url = this_url.clone();
        let method = request.method().clone();
        move |or: (StatusCode, String)| {
            debug!("Re-authenticating user: {}", or.1);
            if method == Method::GET {
                let mut redirect_url = this_url.clone();
                redirect_url.set_path("/auth/login");
                redirect_url.set_query(Some(&format!(
                    "redirect_to={}",
                    urlencoding::encode(this_url.as_ref())
                )));

                let headers = {
                    let mut headers = HeaderMap::new();
                    headers.insert("X-Auth-Error", format!("{:?}", or.1).parse().unwrap());
                    headers
                };

                (headers, Redirect::to(redirect_url.as_ref())).into_response()
            } else {
                or.into_response()
            }
        }
    };

    let mut auth_cookie = match AuthCookie::get_from_jar(&jar) {
        Ok(cookie) => cookie,
        Err(err) => return re_authenticate((StatusCode::UNAUTHORIZED, err)),
    };

    let nonce = match jar.get("nonce") {
        Some(cookie) => Nonce::new(cookie.value_trimmed().to_string()),
        None => {
            return re_authenticate((StatusCode::UNAUTHORIZED, "Nonce cookie missing".to_string()))
        }
    };

    let id_token_claims = match auth_cookie
        .id_token
        .claims(&state.oidc_client.id_token_verifier(), &nonce)
    {
        Ok(claims) => claims.clone(),
        Err(_) => match if state.oidc_client.token_uri().is_some() {
            debug!("Refreshing user's tokens");
            match state
                .oidc_client
                .exchange_refresh_token(&auth_cookie.refresh_token)
                .expect("no token endpoint")
                .request_async(&state.http_client)
                .await
            {
                Ok(token_response) => {
                    match process_token_response(
                        &state.oidc_client,
                        &state.http_client,
                        &jar,
                        token_response,
                    )
                    .await
                    {
                        Ok((new_auth_cookie, claims)) => {
                            auth_cookie = new_auth_cookie;
                            jar = auth_cookie.add_to_jar(jar);
                            Some(claims)
                        }
                        Err(_) => None,
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        } {
            Some(claims) => claims,
            None => {
                return re_authenticate((
                    StatusCode::UNAUTHORIZED,
                    "ID token verification failed".to_string(),
                ));
            }
        },
    };

    let group_id_token_claims = match merge_claims(&id_token_claims, &auth_cookie.additional_claims)
    {
        Ok(group_id_token_claims) => group_id_token_claims,
        Err(err) => {
            return re_authenticate((StatusCode::INTERNAL_SERVER_ERROR, err.to_string()));
        }
    };

    request.extensions_mut().insert(group_id_token_claims);

    let response = next.run(request).await;

    (jar, response).into_response()
}

pub mod api {
    use std::{borrow::Cow, str::FromStr};

    use axum::{
        extract::{Query, State},
        response::{IntoResponse, Redirect},
    };
    use axum_extra::extract::PrivateCookieJar;
    use cookie::{time::Duration, Cookie};
    use openidconnect::{
        core::{CoreIdToken, CoreResponseType},
        AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, RedirectUrl, Scope,
    };
    use reqwest::StatusCode;
    use serde::Deserialize;
    use utoipa::IntoParams;
    use utoipa_axum::{router::OpenApiRouter, routes};

    use crate::{extract_url::ExtractUrl, state::AppState};

    use super::{process_token_response, ProcessTokenResponseError};

    pub fn router() -> OpenApiRouter<AppState> {
        OpenApiRouter::new()
            .routes(routes!(login))
            .routes(routes!(callback))
    }

    #[derive(Debug, Deserialize, IntoParams)]
    struct LoginQueryParams {
        redirect_to: Option<String>,
    }

    /// OpenID Connect login endpoint
    #[utoipa::path(
        method(get),
        path = "/login",
        params(LoginQueryParams),
        responses(
            (
                status = SEE_OTHER,
                description = "Redirect to OIDC provider's authorization endpoint", 
                headers(
                    ("location" = String, description = "The URL the user should visit to authorize the app")
                )
            )
        )
    )]
    async fn login(
        State(state): State<AppState>,
        ExtractUrl(this_url): ExtractUrl,
        Query(params): Query<LoginQueryParams>,
        jar: PrivateCookieJar,
    ) -> (PrivateCookieJar, Redirect) {
        let redirect_url = RedirectUrl::from_url(this_url.join("callback").expect("invalid URL"));

        let mut authorization_request = state
            .oidc_client
            .authorize_url(
                AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            )
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            .add_scope(Scope::new("offline_access".to_string()))
            .set_redirect_uri(Cow::Borrowed(&redirect_url));

        let id_token;
        if let Some(id_token_cookie) = jar
            .get("id_token")
            .and_then(|cookie| CoreIdToken::from_str(cookie.value_trimmed()).ok())
        {
            id_token = id_token_cookie.clone();
            authorization_request = authorization_request.set_id_token_hint(&id_token);
        }

        let (authorize_url, csrf_state, nonce) = authorization_request.url();

        let jar = jar
            .add(Cookie::new("csrf_state", csrf_state.secret().clone()))
            .add(
                Cookie::build(("nonce", nonce.secret().clone()))
                    .path("/")
                    .max_age(Duration::days(14)),
            );

        let jar = match params.redirect_to {
            Some(redirect_to) => jar.add(Cookie::new("login_redirect_to", redirect_to)),
            None => jar.remove(Cookie::from("login_redirect_to")),
        };

        (jar, Redirect::to(authorize_url.as_str()))
    }

    #[derive(Debug, Deserialize, IntoParams)]
    struct CallbackQueryParams {
        code: String,
        state: String,
    }

    /// OpenID Connect callback endpoint
    #[utoipa::path(
        method(get),
        path = "/callback",
        params(CallbackQueryParams),
        responses(
            (
                status = SEE_OTHER,
                description = "Redirect to homepage", 
                headers(
                    ("location" = String, description = "Homepage URL")
                )
            )
        )
    )]
    async fn callback(
        State(state): State<AppState>,
        Query(params): Query<CallbackQueryParams>,
        jar: PrivateCookieJar,
    ) -> impl IntoResponse {
        let csrf_state = CsrfToken::new(params.state);

        match jar.get("csrf_state") {
            None => return (StatusCode::BAD_REQUEST, "CSRF state missing").into_response(),
            Some(cookie) => {
                if cookie.value_trimmed() != csrf_state.secret() {
                    return (StatusCode::BAD_REQUEST, "CSRF state mismatch").into_response();
                }
            }
        }

        let jar = jar.remove(Cookie::from("csrf_state"));

        let code = AuthorizationCode::new(params.code);

        let token_response = match state
            .oidc_client
            .exchange_code(code)
            .expect("no user info endpoint")
            .request_async(&state.http_client)
            .await
        {
            Ok(token_response) => token_response,
            Err(err) => return (StatusCode::BAD_GATEWAY, err.to_string()).into_response(),
        };

        let (auth_cookie, _) = match process_token_response(
            &state.oidc_client,
            &state.http_client,
            &jar,
            token_response,
        )
        .await
        {
            Ok(auth_cookie) => auth_cookie,
            Err(ProcessTokenResponseError::MissingNonce) => {
                return (StatusCode::BAD_REQUEST, "Nonce missing").into_response()
            }
            Err(ProcessTokenResponseError::MissingIdToken) => {
                return (StatusCode::BAD_GATEWAY, "ID token missing").into_response()
            }
            Err(ProcessTokenResponseError::MissingRefreshToken) => {
                return (StatusCode::BAD_GATEWAY, "Refresh token missing").into_response()
            }
            Err(ProcessTokenResponseError::IdTokenError(err)) => {
                return (StatusCode::BAD_GATEWAY, err.to_string()).into_response()
            }
            Err(ProcessTokenResponseError::UserInfoError(err)) => {
                return (StatusCode::BAD_GATEWAY, err.to_string()).into_response()
            }
        };

        let jar = auth_cookie.add_to_jar(jar);

        let redirect_to = match jar.get("login_redirect_to") {
            Some(cookie) => cookie.value_trimmed().to_string(),
            None => "/".to_string(),
        };

        let jar = jar.remove(Cookie::from("login_redirect_to"));

        (jar, Redirect::to(&redirect_to)).into_response()
    }
}
