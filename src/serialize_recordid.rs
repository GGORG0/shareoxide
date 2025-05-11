use serde::{Serialize as _, Serializer};
use surrealdb::RecordId;

pub fn serialize_recordid<S>(record_id: &RecordId, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&record_id.to_string())
}

#[expect(clippy::ptr_arg)]
pub fn serialize_recordid_vec<S>(ids: &Vec<RecordId>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    strings.serialize(serializer)
}
