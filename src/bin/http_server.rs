use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::put,
    Router,
};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::routing::delete;
use kv_store::btree::BTree;
use kv_store::btree::btree::BTreeLogItem;
use kv_store::btree::page_managers::persistent_page_manager::{syncing_loop, PageManagerLogItem, PersistentPageManager};
use kv_store::engine::StorageEngine;
use kv_store::errors::KvResult;
use kv_store::logging::{ItemLogger, MessageItem};

pub const LOG_FOLDER: &str = "logs";

#[derive(Clone)]
enum DurabilityMode {
    AlwaysSync,
    NeverSync,
    PeriodicSync,
}

#[derive(Clone)]
struct AppState {
    engine: Arc<dyn StorageEngine>,
    durability_mode: DurabilityMode
}

fn maybe_sync(engine: &Arc<dyn StorageEngine>, durability_mode: DurabilityMode) -> KvResult<()> {
    match durability_mode {
        DurabilityMode::AlwaysSync => {
            engine.sync()
        },
        DurabilityMode::PeriodicSync => {
            Ok(())
        },
        DurabilityMode::NeverSync => {
            Ok(())
        }
    }
}

async fn set_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
    body: String,
) -> impl IntoResponse {


    if let Err(err) = state.engine.set(key.as_bytes(), body.as_bytes()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    let post_res = maybe_sync(&state.engine, state.durability_mode);
    if let Err(err) = post_res {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }
    (StatusCode::OK, "".into())
}

async fn get_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {

    let result = state.engine.get(key.as_bytes());

    match result {
        Ok(Some(value)) => (StatusCode::OK, String::from_utf8(value).unwrap()),
        Ok(None) => (StatusCode::NOT_FOUND, "".into()),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn delete_handler(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> impl IntoResponse {

    if let Err(err) = state.engine.delete(key.as_bytes()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    let post_res = maybe_sync(&state.engine, state.durability_mode);
    if let Err(err) = post_res {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    (StatusCode::OK, "".into())

}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let durability_mode = DurabilityMode::NeverSync;

    let page_size = 4096;

    let pm_log_data_path = std::path::Path::new(LOG_FOLDER).join("page_manager_data.csv");
    let pm_data_path_s = pm_log_data_path.to_str().unwrap();
    let pm_data_logger = Arc::new(ItemLogger::<PageManagerLogItem>::new(pm_data_path_s, 1_000_0000).await);

    let pm_messages_path = std::path::Path::new(LOG_FOLDER).join("page_manager_messages.csv");
    let pm_message_path_s = pm_messages_path.to_str().unwrap();
    let message_logger = Arc::new(ItemLogger::<MessageItem>::new(pm_message_path_s, 1_000_0000).await);



    let page_manager = Arc::new(
        PersistentPageManager::new("kv.db", page_size, pm_data_logger, message_logger)
    );
    let pm_copy = page_manager.clone();
    if let DurabilityMode::PeriodicSync = durability_mode {
        tokio::task::spawn(async move {
            syncing_loop(
                pm_copy,
                Duration::from_secs(10),
            ).await
        });
    }

    let tree_log_data_path = std::path::Path::new(LOG_FOLDER).join("tree_operations.csv");
    let tree_data_path_s = tree_log_data_path.to_str().unwrap();
    let tree_data_logger = Arc::new(ItemLogger::<BTreeLogItem>::new(tree_data_path_s, 1_000_000).await);
    let tree = BTree::new(page_manager,page_size, tree_data_logger);


    let state = AppState{
        engine: Arc::new(tree),
        durability_mode
    };

    let app = Router::new()
        .route("/kv/{key}", put(set_handler))
        .route("/kv/{key}", get(get_handler))
        .route("/kv/{key}", delete(delete_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}