use serde::{de, Deserialize, Serialize};
use surrealdb::RecordId;

macro_rules! define_table {
    ($table:ident $(, $field:ident : $ty:ty)*) => {
        paste::paste! {
            pub const [<$table:upper>] : &str = stringify!($table);

            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub struct [<$table:camel Data>] {
                $(pub $field: $ty,)*
            }

            #[derive(Debug, serde::Serialize, serde::Deserialize)]
            pub struct [<$table:camel>] {
                pub id: surrealdb::RecordId,
                $(pub $field: $ty,)*
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

define_table!(user, name: String, email: String);
define_table!(link, url: String);
define_table!(file);
define_table!(paste, content: String, language: String);
define_table!(short_link, link: String);
