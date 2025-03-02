use axum::{
    extract::{FromRequestParts, OriginalUri},
    http::request::Parts,
    response::{IntoResponse, Response},
};
use axum_extra::extract::Host;
use reqwest::StatusCode;
use url::Url;

pub struct ExtractUrl(pub Url);

impl<S> FromRequestParts<S> for ExtractUrl
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let OriginalUri(original_uri) = OriginalUri::from_request_parts(parts, state)
            .await
            .map_err(|err| err.into_response())?;

        let uri_parts = original_uri.into_parts();

        let Host(host) = Host::from_request_parts(parts, state)
            .await
            .map_err(|err| err.into_response())?;

        let url = Url::parse(&format!(
            "{}://{}{}",
            uri_parts
                .scheme
                .map(|scheme| scheme.to_string())
                .unwrap_or("http".to_string()),
            uri_parts
                .authority
                .map(|authority| authority.to_string())
                .unwrap_or(host),
            uri_parts
                .path_and_query
                .map(|path_and_query| path_and_query.to_string())
                .unwrap_or("".to_string())
        ))
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response())?;

        Ok(Self(url))
    }
}
