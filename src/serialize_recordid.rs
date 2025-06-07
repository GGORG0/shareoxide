use paste::paste;
use serde::{Serialize as _, Serializer};
use surrealdb::RecordId;

pub fn serialize_recordid_as_key<S>(id: &RecordId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&id.key().to_string())
}

#[expect(clippy::ptr_arg)]
pub fn serialize_recordid_vec_as_key<S>(
    ids: &Vec<RecordId>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let strings: Vec<String> = ids.iter().map(|id| id.key().to_string()).collect();
    strings.serialize(serializer)
}

#[macro_export]
macro_rules! generate_recordid_deserializer {
    ($table:ident) => {
        paste! {
            pub fn [<deserialize_recordid_from_key_for_ $table>]<'de, D>(deserializer: D) -> Result<surrealdb::RecordId, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct RecordIdVisitor;
                impl<'de> serde::de::Visitor<'de> for RecordIdVisitor {
                    type Value = surrealdb::RecordId;
                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a string representing a RecordId key")
                    }
                    fn visit_str<E>(self, v: &str) -> Result<surrealdb::RecordId, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(surrealdb::RecordId::from_table_key(
                            stringify!($table).to_string(),
                            v.to_string(),
                        ))
                    }
                }
                deserializer.deserialize_str(RecordIdVisitor)
            }

            pub fn [<deserialize_recordid_vec_from_key_for_ $table>]<'de, D>(deserializer: D) -> Result<Vec<surrealdb::RecordId>, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct RecordIdVecVisitor;
                impl<'de> serde::de::Visitor<'de> for RecordIdVecVisitor {
                    type Value = Vec<surrealdb::RecordId>;
                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str("a sequence of strings representing RecordId keys")
                    }
                    fn visit_seq<A>(self, mut seq: A) -> Result<Vec<surrealdb::RecordId>, A::Error>
                    where
                        A: serde::de::SeqAccess<'de>,
                    {
                        let mut vec = Vec::new();
                        while let Some(value) = seq.next_element::<String>()? {
                            let id = surrealdb::RecordId::from_table_key(
                                stringify!($table).to_string(),
                                value,
                            );
                            vec.push(id);
                        }
                        Ok(vec)
                    }
                }
                deserializer.deserialize_seq(RecordIdVecVisitor)
            }
        }
    };
}

// TODO: improve this ig

generate_recordid_deserializer!(link);
