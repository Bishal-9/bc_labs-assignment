
use sqlx::FromRow;

#[derive(Clone, FromRow, Debug)]
pub struct RequestFile {
    pub name: String,
    pub format: String,
    pub space: i64,
    pub chunk_data: Vec<Vec<u8>>
}

#[derive(Clone, FromRow, Debug)]
pub struct FileMetadata {
    pub id: i32,
    pub name: String,
    pub format: String,
    pub space: i64,
    pub chunk_ids: Vec<i32>
}

#[allow(dead_code)]
#[derive(Clone, FromRow, Debug)]
pub struct FileChunk {
    pub id: i32,
    pub file_id: i32,
    pub alignment: i32,
    pub chunk_data: Vec<u8>
}
