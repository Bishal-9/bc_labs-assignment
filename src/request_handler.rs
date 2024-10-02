use actix_multipart::Multipart;
use actix_web::{
    web::{Bytes, Path},
    Error, HttpResponse, Responder,
};
use futures::stream;

use crate::utility::{
    get_all_files, get_file_chunks, get_file_metadata, process_multipart_data,
    upload_file_in_chunks,
};

// Route to upload a file
pub async fn handle_upload(payload: Multipart) -> impl Responder {
    match process_multipart_data(payload).await {
        Ok(Some(file_info)) => match upload_file_in_chunks(file_info).await {
            Ok(message) => HttpResponse::Ok().body(message),
            Err(e) => {
                eprintln!("Error uploading file: {:?}", e);
                HttpResponse::InternalServerError().body("Error uploading file")
            }
        },
        Ok(None) => HttpResponse::BadRequest().body("No file found in the upload"),
        Err(_) => HttpResponse::InternalServerError().body("Error processing the upload"),
    }
}

// Route to list all files
pub async fn list_files() -> Result<impl Responder, Error> {
    let files_list = get_all_files().await;

    match files_list {
        Err(e) => {
            // Handle any errors that occurred while fetching the file list
            let error_response = format!("Error: {}", e);
            Ok(HttpResponse::InternalServerError().body(error_response))
        }
        Ok(files) => {
            // Create a response string
            let mut response = String::new();

            // Append file information to the response
            if files.is_empty() {
                response.push_str("No files found.\n");
            } else {
                for file in files {
                    response.push_str(&format!(
                        "ID: {}, Name: {}, Format: {}, Size: {}\n",
                        file.id, file.name, file.format, file.space
                    ));
                }
            }

            Ok(HttpResponse::Ok().content_type("text/plain").body(response))
        }
    }
}

// Route to download a file in chunks
pub async fn handle_download(file_id: Path<i32>) -> Result<impl Responder, Error> {
    // Fetch file metadata from the database
    let file_metadata = match get_file_metadata(file_id.into_inner()).await {
        Ok(metadata) => metadata,
        Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
    };

    // Stream the file chunks
    let mut chunks = match get_file_chunks(file_metadata.id).await {
        Ok(_chunks) => _chunks,
        Err(_) => return Ok(HttpResponse::InternalServerError().finish()),
    };

    if chunks.is_empty() {
        return Ok(HttpResponse::NotFound().body("File not found or empty."));
    }

    // Sort the chunks by the alignment field in increasing order
    chunks.sort_by_key(|chunk| chunk.alignment);

    let _stream = stream::iter(
        chunks
            .into_iter()
            .map(|chunk| Ok::<_, std::io::Error>(Bytes::from(chunk.chunk_data))),
    );

    let response = HttpResponse::Ok()
        .content_type(format!("application/octet-stream"))
        .insert_header((
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", file_metadata.name),
        ))
        .insert_header(("Content-Length", file_metadata.space.to_string()))
        .streaming(_stream);

    Ok(response)
}
