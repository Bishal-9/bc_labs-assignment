use lazy_static::lazy_static;
use sqlx::{Pool, Postgres};

use std::sync::{Arc, Mutex};

lazy_static! (
    static ref DATABASE_POOL: Arc<Mutex<Option<Pool<Postgres>>>> = Arc::new(Mutex::new(None));
);

pub fn get_connection_pool() -> Option<Pool<Postgres>> {
    if DATABASE_POOL.is_poisoned() {
        DATABASE_POOL.clear_poison();
    }

    Arc::clone(&DATABASE_POOL)
        .lock()
        .expect("Database Connection Pool locking error")
        .clone()
}

pub fn set_connection_pool(_new_connection_pool: Pool<Postgres>) {
    if DATABASE_POOL.is_poisoned() {
        DATABASE_POOL.clear_poison();
    }

    *DATABASE_POOL.lock().expect("msg") = Some(_new_connection_pool);
}
