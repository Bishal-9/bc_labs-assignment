use actix_multipart::Multipart;
use actix_web::Error;
use futures::{future::join_all, StreamExt};
use sqlx::{postgres::PgPoolOptions, query, query_as, Error as SqlxError, Pool, Postgres};
use std::{env::var, error};
use tokio::{runtime::Handle, task::spawn_blocking};

use crate::{
    global_data::{get_connection_pool, set_connection_pool},
    structure::{FileChunk, FileMetadata, RequestFile},
};

pub async fn establish_database_connection() {
    // Get the DATABASE_URL from the environment variables
    let database_url = var("DATABASE_URL").expect("Provide Database URL");

    // Minimum Database Connection Pool
    let min_connection_pool = 2;

    // Maximum Database Connection Pool
    let max_connection_pool = 3;

    // Create a connection pool for PostgreSQL
    let connection_pool: Pool<Postgres> = PgPoolOptions::new()
        .min_connections(min_connection_pool)
        .max_connections(max_connection_pool)
        .connect(&database_url)
        .await
        .expect("Establishing Database Connection Pool Error");

    set_connection_pool(connection_pool);
}

pub async fn table_exist() {
    let database_pool = get_connection_pool().expect("Expecting a Database");

    query(
        r#"
            CREATE TABLE IF NOT EXISTS file_meta (
                id SERIAL PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                format VARCHAR(50) NOT NULL,
                space BIGINT NOT NULL,
                chunk_ids INT[] NOT NULL
            );
        "#,
    )
    .execute(&database_pool)
    .await
    .expect("Error creating/checking file_meta table in database");

    query(
        r#"
            CREATE TABLE IF NOT EXISTS file_chunk (
                id SERIAL PRIMARY KEY,
                file_id INT REFERENCES file_meta(id) ON DELETE CASCADE,
                alignment INT,
                chunk_data BYTEA NOT NULL
            );
        "#,
    )
    .execute(&database_pool)
    .await
    .expect("Error creating/checking file_chunks table in database");
}

pub async fn get_all_files() -> Result<Vec<FileMetadata>, Box<dyn error::Error>> {
    let database_pool =
        get_connection_pool().ok_or_else(|| String::from("Database Connection Error"))?;

    // Query to fetch all file metadata
    let files = query_as::<_, FileMetadata>(
        r#"
            SELECT * FROM file_meta
        "#,
    )
    .fetch_all(&database_pool)
    .await?;

    Ok(files)
}

// Helper function to extract file metadata from multipart data
pub async fn process_multipart_data(mut payload: Multipart) -> Result<Option<RequestFile>, Error> {
    while let Some(item) = payload.next().await {
        match item {
            Ok(mut field) => {
                match field.content_disposition() {
                    Some(content_disposition) => {
                        // Check if this field is for the file
                        if content_disposition.get_name() == Some("file") {
                            // Get the file name
                            let file_name = content_disposition
                                .get_filename()
                                .unwrap_or("unknown")
                                .to_string();

                            // Determine the file format from the file name
                            let file_format =
                                file_name.split('.').last().unwrap_or("unknown").to_string();

                            let mut file_data = Vec::new();
                            let mut chunks = Vec::new();
                            let chunk_size = 1 * 1024 * 1024; // 1MB per chunk, you can adjust this size

                            while let Some(chunk) = field.next().await {
                                match chunk {
                                    Ok(bytes) => {
                                        file_data.extend_from_slice(&bytes);

                                        // Check if we need to process the chunk
                                        if file_data.len() >= chunk_size {
                                            // Push the chunk data to the chunk list
                                            chunks.push(file_data.clone()); // Save the chunk
                                            file_data.clear(); // Clear the buffer for the next chunk
                                        }
                                    }
                                    Err(_) => {
                                        // Handle chunk read error
                                        return Ok(None);
                                    }
                                }
                            }
                            let file_size = file_data.len() as i64;
                            // If there's any remaining data in file_data after the loop, push it as the final chunk
                            if !file_data.is_empty() {
                                chunks.push(file_data);
                            }

                            return Ok(Some(RequestFile {
                                name: file_name,
                                format: file_format,
                                space: file_size,
                                chunk_data: chunks,
                            }));
                        }
                    }
                    None => return Ok(None),
                }
            }
            Err(_) => {
                // Handle field read error if necessary
                return Ok(None);
            }
        }
    }

    Ok(None) // If no file is found, return None
}

// Multithreaded async function to upload file chunks to the database
pub async fn upload_file_in_chunks(
    file_info: RequestFile,
) -> Result<String, Box<dyn error::Error>> {
    let file_name = file_info.name.clone();
    let file_format = file_info.format.clone();
    let file_space = file_info.space;

    let database_pool =
        get_connection_pool().ok_or_else(|| String::from("Database Connection Error"))?;

    // First, insert the file metadata and get the file_id
    let row: (i32,) = query_as(
        "INSERT INTO file_meta (name, format, space, chunk_ids) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(&file_name)
    .bind(&file_format)
    .bind(file_space)
    .bind(&Vec::<i32>::new())
    .fetch_one(&database_pool)
    .await?;

    let file_id = row.0; // Extract the file ID

    let mut upload_tasks = Vec::new();
    let mut chunk_ids = Vec::new();

    // Iterate through each chunk and spawn a new task to upload it
    for (index, chunk) in file_info.chunk_data.iter().enumerate() {
        let _alignment = index + 1;
        let file_id_clone = file_id;
        let chunk_clone = chunk.clone();
        let pool_clone = database_pool.clone();

        let task = spawn_blocking(move || {
            // Perform blocking operation to upload chunk
            let result = Handle::current().block_on(async move {
                let result: (i32, ) = query_as(
                    "INSERT INTO file_chunk (file_id, alignment, chunk_data) VALUES ($1, $2, $3) RETURNING id",
                )
                .bind(file_id_clone)
                .bind(_alignment as i32)
                .bind(chunk_clone)
                .fetch_one(&pool_clone)
                .await?;

                Ok::<i32, SqlxError>(result.0) // Return chunk_id
            });

            result
        });

        upload_tasks.push(task);
    }

    // Wait for all chunk uploads to complete and collect chunk IDs
    let results = join_all(upload_tasks).await;

    for result in results {
        match result {
            Ok(inner_result) => {
                // Handle the inner Result
                match inner_result {
                    Ok(chunk_id) => {
                        chunk_ids.push(chunk_id);
                    }
                    Err(e) => {
                        eprintln!("Error uploading file chunk: {:?}", e);
                        return Ok("Failed to upload file".to_string());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error in thread spawn: {:?}", e);
                return Ok("Failed to upload file".to_string());
            }
        }
    }

    // Update the file_meta table with the collected chunk_ids
    query("UPDATE file_meta SET chunk_ids = $1 WHERE id = $2")
        .bind(&chunk_ids)
        .bind(file_id)
        .execute(&database_pool)
        .await?;

    Ok(format!("File uploaded successfully with ID: {}", file_id))
}

// Function to get file metadata from the database using file_id
pub async fn get_file_metadata(file_id: i32) -> Result<FileMetadata, Box<dyn error::Error>> {
    let database_pool =
        get_connection_pool().ok_or_else(|| String::from("Database Connection Error"))?;

    let file_meta: FileMetadata = query_as("SELECT * FROM file_meta WHERE id = $1")
        .bind(file_id)
        .fetch_one(&database_pool) // Replace with your actual connection pool
        .await?;

    Ok(file_meta)
}

// Function to retrieve all chunks for a given file ID
pub async fn get_file_chunks(file_id: i32) -> Result<Vec<FileChunk>, SqlxError> {
    let database_pool =
        get_connection_pool().expect("Expecting an Active Database Connection Error");

    query_as::<_, FileChunk>("SELECT * FROM file_chunk WHERE file_id = $1 ORDER BY alignment")
        .bind(file_id)
        .fetch_all(&database_pool) // Adjust to your DB connection pool
        .await
}
