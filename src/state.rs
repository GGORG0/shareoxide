use std::{ops::Deref, sync::Arc};

use axum::extract::FromRef;
use surrealdb::{engine::any::Any, Surreal};

use crate::settings::ArcSettings;

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
    pub settings: ArcSettings,
    pub db: SurrealDb,
}

impl FromRef<AppState> for ArcSettings {
    fn from_ref(state: &AppState) -> Self {
        state.settings.clone()
    }
}

pub type SurrealDb = Surreal<Any>;

impl FromRef<AppState> for SurrealDb {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}
