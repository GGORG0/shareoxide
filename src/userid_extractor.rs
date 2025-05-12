use std::{ops::Deref, str::FromStr};

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use axum_oidc::OidcClaims;
use color_eyre::{eyre::OptionExt, Result};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use tower_sessions::Session;
use tracing::{error, warn};

use crate::{
    schema::{PartialUser, User},
    state::{AppState, SurrealDb},
    GroupClaims,
};

const USER_ID_KEY: &str = "user_id";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SessionUserId(pub RecordId);

impl SessionUserId {
    pub async fn from_session(session: Session) -> Result<Option<Self>> {
        session
            .get::<String>(USER_ID_KEY)
            .await?
            .map(|id| id.parse())
            .transpose()
    }

    pub async fn from_claims(
        claims: &OidcClaims<GroupClaims>,
        db: &SurrealDb,
    ) -> Result<Option<Self>> {
        Ok(
            match db
                .query("SELECT id FROM user WHERE subject = $subject")
                .bind(("subject", claims.subject().clone()))
                .await?
                .take("id")?
            {
                Some(id) => Some(Self(id)),
                None => {
                    let user = PartialUser {
                        subject: claims.subject().deref().clone(),
                        email: claims.email().unwrap().deref().clone(),
                        name: claims.name().unwrap().get(None).unwrap().to_string(),
                    };

                    db.create::<Option<User>>("user")
                        .content(user)
                        .await?
                        .map(|user| Self(user.id))
                }
            },
        )
    }

    pub async fn from_session_or_claims(
        session: Session,
        claims: &OidcClaims<GroupClaims>,
        db: &SurrealDb,
    ) -> Result<Self> {
        match Self::from_session(session.clone()).await? {
            Some(value) => Ok(value),
            None => {
                warn!("User id not found in session");

                Self::from_claims(claims, db)
                    .await?
                    .inspect(|userid| {
                        let userid = userid.clone();
                        tokio::task::spawn(async move { userid.to_session(session).await });
                    })
                    .or_else(|| {
                        warn!("User id not found in claims");
                        None
                    })
                    .ok_or_eyre("Failed to get user id")
            }
        }
    }

    pub async fn to_session(&self, session: Session) -> Result<(), tower_sessions::session::Error> {
        session.insert(USER_ID_KEY, self.0.to_string()).await
    }
}

impl From<RecordId> for SessionUserId {
    fn from(value: RecordId) -> Self {
        Self(value)
    }
}

impl From<SessionUserId> for RecordId {
    fn from(value: SessionUserId) -> Self {
        value.0
    }
}

impl FromStr for SessionUserId {
    type Err = color_eyre::Report;

    fn from_str(s: &str) -> Result<Self> {
        Ok(RecordId::from_str(s).map(SessionUserId)?)
    }
}

impl Deref for SessionUserId {
    type Target = RecordId;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequestParts<AppState> for SessionUserId {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                (
                    StatusCode::UNAUTHORIZED,
                    "Failed to extract session from request",
                )
            })?;

        let claims = OidcClaims::<GroupClaims>::from_request_parts(parts, state)
            .await
            .map_err(|e| {
                error!(error = ?e, "Failed to extract token claims from request");
                (StatusCode::UNAUTHORIZED, "Failed to extract token claims")
            })?;

        Self::from_session_or_claims(session, &claims, &state.db)
            .await
            .map_err(|e| {
                error!(error = ?e, "Failed to get user id from session or claims");
                (StatusCode::UNAUTHORIZED, "Failed to get user id")
            })
    }
}
