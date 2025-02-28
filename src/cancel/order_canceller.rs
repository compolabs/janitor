use std::sync::Arc;
use tokio::sync::{broadcast::Receiver, Semaphore};
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};

use crate::storage::market_storage::TradingEngine;
use crate::cancel::cancel_processor::CancelProcessor;

const CHECK_INTERVAL: Duration = Duration::from_secs(3);
const CANCEL_BLOCK_THRESHOLD: i64 = 10;


pub async fn start_order_canceller(
    trading_engine: Arc<TradingEngine>,
    cancel_processor: Arc<CancelProcessor>,
    mut shutdown: Receiver<()>,
) {
    
    let wallet_semaphore = Arc::new(Semaphore::new(5));

    loop {
        tokio::select! {
            
            _ = shutdown.recv() => {
                info!("🛑 Order canceller received shutdown signal. Exiting...");
                break;
            }
            
            _ = sleep(CHECK_INTERVAL) => {
                info!("🔍 Checking market orders for potential cancellation...");
                if let Err(e) = check_market_orders(&trading_engine, &cancel_processor, &wallet_semaphore).await {
                    error!("❌ Error while checking orders: {}", e);
                }
            }
        }
    }
}


async fn check_market_orders(
    trading_engine: &TradingEngine,
    cancel_processor: &CancelProcessor,
    wallet_semaphore: &Arc<Semaphore>,
) -> Result<(), String> {
    
    let current_block = cancel_processor
        .get_current_block()
        .await
        .map_err(|e| format!("Failed to get block: {}", e))?;

    for (symbol, queue) in &trading_engine.markets {
        let orders = queue.get_all_orders();

        if orders.is_empty() {
            info!("🟢 No active market orders for {}", symbol);
            continue;
        }

        info!("📌 Active market orders for {}:", symbol);
        for order in orders {
            let blocks_passed = current_block - order.block_number;

            info!(
                "- Order ID: {}, Blocks Passed: {}",
                order.id, blocks_passed
            );

            
            if blocks_passed >= CANCEL_BLOCK_THRESHOLD {
                
                if queue.mark_as_cancelling(&order.id) {
                    
                    match wallet_semaphore.clone().try_acquire_owned() {
                        Ok(permit) => {
                            info!("🚀 Sending order {} for cancellation", order.id);

                            
                            let cancel_processor_clone = cancel_processor.clone();
                            let order_clone = order.clone();

                            tokio::spawn(async move {
                                
                                if let Err(e) = cancel_processor_clone.cancel_order(order_clone).await {
                                    error!("❌ Failed to cancel order: {}", e);
                                }
                                
                                drop(permit);
                            });
                        }
                        Err(_) => {
                            warn!("No available wallets. Skipping cancellation for {}", order.id);
                        }
                    }
                } else {
                    warn!("Order {} is already in cancellation queue", order.id);
                }
            }
        }
    }

    Ok(())
}
