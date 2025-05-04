use std::{ops::Deref, sync::Arc};

use surrealdb::{engine::any::Any, Surreal};

use crate::settings::Settings;

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
    pub settings: Arc<Settings>,
    pub db: Surreal<Any>,
}
