use std::ops::Deref;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use axum_oidc::OidcClaims;
use color_eyre::{eyre::OptionExt, Result};
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use tower_sessions::Session;

use crate::{
    schema::{DatabaseObject, DatabaseObjectData, User, UserData},
    state::{AppState, SurrealDb},
    GroupClaims,
};

const USER_ID_KEY: &str = "user_id";

#[derive(Debug, Deserialize, Serialize)]
pub struct SessionUserId(pub RecordId);

impl SessionUserId {
    pub async fn from_session(
        session: &Session,
    ) -> Result<Option<Self>, tower_sessions::session::Error> {
        session.get::<Self>(USER_ID_KEY).await
    }

    pub async fn from_claims(
        claims: &OidcClaims<GroupClaims>,
        db: &SurrealDb,
    ) -> Result<Option<Self>, surrealdb::Error> {
        Ok(
            match db
                .query("SELECT id FROM type::table($table) WHERE subject = $subject")
                .bind(("table", User::TABLE))
                .bind(("subject", claims.subject().clone()))
                .await?
                .take("id")?
            {
                Some(id) => Some(Self(id)),
                None => {
                    let user = UserData {
                        subject: claims.subject().deref().clone(),
                        email: claims.email().unwrap().deref().clone(),
                        name: claims.name().unwrap().get(None).unwrap().to_string(),
                    };

                    user.create(db)
                        .await
                        .map_err(|err| *err)?
                        .map(|user| Self(user.id))
                }
            },
        )
    }

    pub async fn from_session_or_claims(
        session: &Session,
        claims: &OidcClaims<GroupClaims>,
        db: &SurrealDb,
    ) -> Result<Self> {
        match Self::from_session(session).await? {
            Some(value) => Ok(value),
            None => Self::from_claims(claims, db)
                .await?
                .ok_or_eyre("Failed to get user id"),
        }
    }

    pub async fn to_session(
        &self,
        session: &mut Session,
    ) -> Result<(), tower_sessions::session::Error> {
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
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Failed to extract token claims"))?;

        Self::from_session_or_claims(&session, &claims, &state.db)
            .await
            .map_err(|_| (StatusCode::UNAUTHORIZED, "Failed to get user id"))
    }
}
