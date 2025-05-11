use serde::{Serialize as _, Serializer};
use surrealdb::RecordId;

pub fn serialize_recordid<S>(id: &RecordId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&id.to_string())
}

pub fn serialize_recordid_as_key<S>(id: &RecordId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&id.key().to_string())
}

#[expect(clippy::ptr_arg)]
pub fn serialize_recordid_vec<S>(ids: &Vec<RecordId>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    strings.serialize(serializer)
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
