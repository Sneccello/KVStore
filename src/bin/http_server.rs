use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::put,
    Router,
};
use std::sync::{Arc, Mutex};
use axum::response::IntoResponse;
use axum::routing::get;
use kv_store::btree::BTree;
use kv_store::btree::page_manager::PersistentPageManager;
use kv_store::engine::StorageEngine;

#[derive(Clone)]
struct AppState {
    engine: Arc<Mutex<dyn StorageEngine + Send>>
}

async fn set_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
    body: String,
) -> StatusCode {
    let mut engine = match state.engine.lock(){
        Ok(guard) => guard,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR,
    };

    if let Err(_) = engine.set(key.as_bytes(), body.as_bytes()) {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if let Err(_) = engine.sync() {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}

async fn get_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {
    let engine = match state.engine.lock() {
        Ok(guard) => guard,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let result = engine.get(key.as_bytes());

    match result {
        Ok(Some(value)) => (StatusCode::OK, value).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let page_size = 4096;
    let page_manager = PersistentPageManager::new("kv.db", page_size);
    let tree = BTree::new(Box::new(page_manager),page_size);

    let state = AppState{
        engine: Arc::new(Mutex::new(tree))
    };

    let app = Router::new()
        .route("/kv/{key}", put(set_handler))
        .route("/kv/{key}", get(get_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}