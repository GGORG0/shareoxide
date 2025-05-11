#![allow(dead_code, unused_imports, unused_variables)]

use async_trait::async_trait;
use axum_oidc::OidcClaims;
use serde::{Deserialize, Serialize};
use surrealdb::{engine::any::Any, Datetime, RecordId, RecordIdKey, Surreal};
use utoipa::ToSchema;

use crate::{state::SurrealDb, GroupClaims};

#[async_trait]
pub trait DatabaseObject: Sized + for<'de> Deserialize<'de> + Serialize {
    type DataType: DatabaseObjectData;
    const TABLE: &'static str;

    fn id(&self) -> &RecordId;

    async fn select(
        db: &SurrealDb,
        id: RecordIdKey,
    ) -> Result<Option<Self>, Box<surrealdb::Error>> {
        Ok(db.select((Self::TABLE, id)).await?)
    }

    async fn update(
        &self,
        db: &SurrealDb,
        upsert: bool,
    ) -> Result<Option<Self>, Box<surrealdb::Error>> {
        let serialized = serde_json::to_value(self).map_err(|e| {
            surrealdb::Error::Api(surrealdb::error::Api::SerializeValue(e.to_string()))
        })?;

        Ok(if upsert {
            db.upsert(self.id()).content(serialized).await?
        } else {
            db.update(self.id()).content(serialized).await?
        })
    }

    async fn delete(&self, db: &SurrealDb) -> Result<Option<Self>, Box<surrealdb::Error>> {
        Ok(db.delete(self.id()).await?)
    }
}

#[async_trait]
pub trait DatabaseObjectData: Sized + for<'de> Deserialize<'de> + Serialize {
    type FullType: DatabaseObject;

    async fn create(
        &self,
        db: &SurrealDb,
    ) -> Result<Option<Self::FullType>, Box<surrealdb::Error>> {
        let serialized = serde_json::to_value(self).map_err(|e| {
            surrealdb::Error::Api(surrealdb::error::Api::SerializeValue(e.to_string()))
        })?;

        Ok(db.create(Self::FullType::TABLE).content(serialized).await?)
    }
}

macro_rules! define_table {
    ($table:ident, {
        $( $(#[$meta:meta])* $field:ident : $ty:ty ),* $(,)?
    }) => {
        paste::paste! {
            #[derive(Debug, Serialize, Deserialize)]
            pub struct [<$table:camel>] {
                pub id: RecordId,
                $(
                    $(#[$meta])*
                    pub $field: $ty,
                )*
            }

            impl DatabaseObject for [<$table:camel>] {
                type DataType = [<$table:camel Data>];
                const TABLE: &'static str = stringify!($table);

                fn id(&self) -> &RecordId {
                    &self.id
                }
            }

            #[derive(Debug, Serialize, Deserialize)]
            pub struct [<$table:camel Data>] {
                $(
                    $(#[$meta])*
                    pub $field: $ty,
                )*
            }

            impl DatabaseObjectData for [<$table:camel Data>] {
                type FullType = [<$table:camel>];
            }

            impl From<[<$table:camel>]> for [<$table:camel Data>] {
                fn from(value: [<$table:camel>]) -> Self {
                    Self {
                        $($field: value.$field,)*
                    }
                }
            }
        }
    };
}

define_table!(user, {
    subject: String,
    name: String,
    email: String,
});

define_table!(link, {
    url: String,
});

define_table!(shortcut, {
    link: String,
});

define_table!(expands_to, {
    r#in: RecordId,
    out: RecordId,
});

define_table!(created, {
    r#in: RecordId,
    out: RecordId,
    timestamp: Datetime,
});

impl From<OidcClaims<GroupClaims>> for UserData {
    fn from(claims: OidcClaims<GroupClaims>) -> Self {
        Self {
            subject: claims.subject().to_string(),
            name: claims.name().unwrap().get(None).unwrap().to_string(),
            email: claims.email().unwrap().to_string(),
        }
    }
}
