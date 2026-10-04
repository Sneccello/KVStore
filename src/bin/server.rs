use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::put,
    Router,
};
use std::sync::Arc;
use std::time::Duration;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::routing::delete;
use kv_store::benchmark_utils::{DurabilityMode, TestConfig};
use kv_store::btree::BTree;
use kv_store::btree::btree::BTreeLogItem;
use kv_store::btree::page_managers::persistent_page_manager::{syncing_loop, PageManagerLogItem, PersistentPageManager};
use kv_store::engine::StorageEngine;
use kv_store::errors::{KvError, KvResult};
use kv_store::logging::{ItemLogger};


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


    if let Err(err) = state.engine.set(key.as_bytes(), body.as_bytes()).await {
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

    if let Err(KvError::KeyNotFound(key)) = state.engine.delete(key.as_bytes()).await {
        return (StatusCode::NOT_FOUND, String::from_utf8_lossy(&key).to_string());
    }

    let post_res = maybe_sync(&state.engine, state.durability_mode);
    if let Err(err) = post_res {
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    (StatusCode::OK, "".into())

}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let config = TestConfig::new("configs/wal_write.yaml");

    let pm_log_data_path = std::path::Path::new(&config.log_folder).join("page_manager_data.csv");
    let pm_data_path_s = pm_log_data_path.to_str().unwrap();
    let pm_data_logger = Arc::new(ItemLogger::<PageManagerLogItem>::new(pm_data_path_s, 100_000).await);


    let page_manager = Arc::new(
        PersistentPageManager::new("kv.db", config.server_config.page_size, pm_data_logger, config.server_config.wal_enabled)
    );
    let pm_copy = page_manager.clone();
    if let DurabilityMode::PeriodicSync = config.server_config.durability_mode {
        tokio::task::spawn(async move {
            syncing_loop(
                pm_copy,
                Duration::from_secs(10),
            ).await
        });
    }

    let tree_log_data_path = std::path::Path::new(&config.log_folder).join("tree_operations.csv");
    let tree_data_path_s = tree_log_data_path.to_str().unwrap();
    let tree_data_logger = Arc::new(ItemLogger::<BTreeLogItem>::new(tree_data_path_s, 100_000).await);
    let tree = BTree::new(page_manager,config.server_config.page_size, tree_data_logger);


    let state = AppState{
        engine: Arc::new(tree),
        durability_mode: config.server_config.durability_mode
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