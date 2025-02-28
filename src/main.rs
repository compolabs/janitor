use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task;
use tracing::{error, info};

use error::Error;
use storage::market_storage::TradingEngine;
use indexer::pangea::PangeaIndexer;
use indexer::Indexer;
use cancel::order_canceller::start_order_canceller;
use cancel::cancel_processor::CancelProcessor;

pub mod cancel;
pub mod config;
pub mod error;
pub mod indexer;
pub mod storage;

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();
    info!("Logger initialized");

    
    let config_path = "config.json";
    let configs = match TradingEngine::load_config(config_path) {
        Ok(configs) => configs,
        Err(e) => {
            error!("Failed to load config: {:?}", e);
            return Err(e);
        }
    };

    
    let trading_engine = Arc::new(TradingEngine::new(configs));
    trading_engine.print_markets();

    
    let cancel_processor = Arc::new(CancelProcessor::new());

    
    let (shutdown_tx, _) = broadcast::channel(1);

    
    let indexer = PangeaIndexer::new(Arc::clone(&trading_engine));
    let mut tasks = Vec::new();
    indexer.initialize(&mut tasks).await?;

    
    let trading_engine_clone = Arc::clone(&trading_engine);
    let cancel_processor_clone = Arc::clone(&cancel_processor);
    let shutdown_rx = shutdown_tx.subscribe();
    tasks.push(task::spawn(async move {
        start_order_canceller(trading_engine_clone, cancel_processor_clone, shutdown_rx).await;
    }));

    
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl+C received, shutting down...");
            let _ = shutdown_tx.send(());
        }
        _ = futures_util::future::join_all(tasks) => {
            info!("All tasks completed");
        }
    }

    Ok(())
}
