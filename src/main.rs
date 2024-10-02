mod global_data;
mod request_handler;
mod structure;
mod utility;

use actix_web::{web::{get, post}, App, HttpServer};
use dotenv::dotenv;
use std::env::set_var;
use tokio::runtime::Runtime;

use request_handler::{list_files, handle_upload, handle_download};
use utility::{establish_database_connection, table_exist};
fn main() {
    // Load environment variables from .env file
    dotenv().ok();

    // Configure logging
    set_var("RUST_LOG", "actix_web=info");

    let database_executor = Runtime::new().expect("Failed to create Runtime");
    database_executor.block_on(async move {
        establish_database_connection().await;

        table_exist().await;
    });

    let server_executor = Runtime::new().expect("Failed to create Runtime");
    let _ = server_executor.block_on(async {

        // Start the Actix web server on port 5000 with multi-threading
        HttpServer::new(|| {
            App::new()
                // .wrap(Logger::default()) // Add logging middleware
                .route("/upload", post().to(handle_upload))
                .route("/files", get().to(list_files))
                .route("/download/{file_id}", get().to(handle_download))
        })
        .bind("0.0.0.0:5000")? // Bind to the address and port
        .workers(4) // Specify the number of worker threads
        .run()
        .await
    });
}
