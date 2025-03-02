use std::{fs, ops::Deref, sync::Arc};

use axum::extract::FromRef;
use cookie::Key;

use crate::oidc::OidcClient;

#[derive(Clone)]
pub struct AppState(Arc<InnerState>);

impl AppState {
    pub fn new(state: InnerState) -> Self {
        Self(Arc::new(state))
    }
}

impl Deref for AppState {
    type Target = InnerState;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct InnerState {
    pub key: Key,
    pub oidc_client: OidcClient,
    pub http_client: reqwest::Client,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.0.key.clone()
    }
}

pub trait GetCookieKey {
    fn get_cookie_key() -> Key;
}

impl GetCookieKey for Key {
    fn get_cookie_key() -> Key {
        // TODO: Generate a key and store it in a database
        if fs::exists("cookie_key.bin").expect("failed to check for cookie key") {
            let key = fs::read("cookie_key.bin").expect("failed to read cookie key");
            Key::derive_from(key.as_ref())
        } else {
            let key = Key::generate();
            fs::write("cookie_key.bin", key.master()).expect("failed to write cookie key");
            key
        }
    }
}
