use crate::indexer::spot_order::{LimitType, OrderStatus, OrderType, SpotOrder};
use crate::storage::market_storage::TradingEngine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

#[derive(Debug, Deserialize, Serialize)]
pub struct PangeaOrderEvent {
    pub chain: u64,
    pub block_number: i64,
    pub block_hash: String,
    pub transaction_hash: String,
    pub transaction_index: u64,
    pub log_index: u64,
    pub market_id: String,
    pub order_id: String,
    pub event_type: Option<String>,
    pub asset: Option<String>,
    pub amount: Option<u128>,
    pub asset_type: Option<String>,
    pub order_type: Option<String>,
    pub price: Option<u128>,
    pub user: Option<String>,
    pub order_matcher: Option<String>,
    pub owner: Option<String>,
    pub limit_type: Option<String>,
}

pub async fn handle_order_event(
    trading_engine: Arc<TradingEngine>,
    event: PangeaOrderEvent,
    symbol: String,
) {
    if let Some(event_type) = event.event_type.as_deref() {
        match event_type {
            "Open" => handle_open_order(trading_engine, event, symbol).await,
            "Trade" => handle_trade_event(trading_engine, event, symbol).await,
            "Cancel" => handle_cancel_event(trading_engine, event, symbol).await,
            _ => warn!("Unknown event type: {:?}", event),
        }
    } else {
        error!("❌ Not event type: {:?}", event);
    }
}

async fn handle_open_order(
    trading_engine: Arc<TradingEngine>,
    event: PangeaOrderEvent,
    symbol: String,
) {
    if let Some(order) = create_new_order_from_event(&event) {
        if let Some(queue) = trading_engine.get_market_queue(&symbol) {
            queue.add_order(order);
            info!("🟢 New MKT order added: {}", event.order_id);
        } else {
            warn!("❌ no market buffer{}", symbol);
        }
    }
}

pub async fn handle_trade_event(
    trading_engine: Arc<TradingEngine>,
    event: PangeaOrderEvent,
    symbol: String,
) {
    let match_size = match event.amount {
        Some(sz) => sz,
        None => {
            warn!("Trade event without amount: {:?}", event);
            return;
        }
    };

    if let Some(queue) = trading_engine.get_market_queue(&symbol) {
        let orders = queue.get_all_orders();
        if let Some(pos) = orders.iter().position(|o| o.id == event.order_id) {
            let mut order = orders[pos].clone();

            if order.amount > match_size {
                order.amount -= match_size;
            } else {
                order.amount = 0;
            }

            match order.limit_type {
                
                Some(LimitType::GTC) | Some(LimitType::MKT) => {
                    if order.amount == 0 {
                        queue.remove_order(&event.order_id);
                        info!("GTC/MKT order fully executed, removed: {}", event.order_id);
                    } else {
                        let mut updated_order = order.clone();
                        updated_order.status = Some(OrderStatus::PartiallyMatched);
                        queue.update_order(updated_order); 
                        info!(
                            "GTC/MKT order partially executed. Left amount={}, ID: {}",
                            order.amount, order.id
                        );
                    }
                }
                
                Some(LimitType::IOC) | Some(LimitType::FOK) => {
                    queue.remove_order(&event.order_id);
                    info!("IOC/FOK order removed after trade: {}", event.order_id);
                }
                None => {
                    queue.remove_order(&event.order_id);
                    warn!("Unknown limit_type, removing order {}", event.order_id);
                }
            }
        } else {
            debug!("Trade event for order not found in queue: {}", event.order_id);
        }
    } else {
        warn!("No market queue for symbol: {}", symbol);
    }
}

async fn handle_cancel_event(
    trading_engine: Arc<TradingEngine>,
    event: PangeaOrderEvent,
    symbol: String,
) {
    if let Some(queue) = trading_engine.get_market_queue(&symbol) {
        queue.remove_order(&event.order_id);
        debug!("🚫 {:?}, order canceled: {}", &symbol, event.order_id);
    }
}

fn create_new_order_from_event(event: &PangeaOrderEvent) -> Option<SpotOrder> {
    if let (Some(price), Some(amount), Some(order_type), Some(limit_type), Some(user)) = (
        event.price,
        event.amount,
        event.order_type.as_deref(),
        event.limit_type.as_deref(),
        &event.user,
    ) {
        let order_type_enum = match order_type {
            "Buy" => OrderType::Buy,
            "Sell" => OrderType::Sell,
            _ => return None,
        };

        let limit_type_enum = match limit_type {
            "MKT" => Some(LimitType::MKT),
            _ => return None, 
        };

        Some(SpotOrder {
            id: event.order_id.clone(),
            user: user.clone(),
            asset: event.asset.clone().unwrap_or_default(),
            amount,
            price,
            timestamp: Utc::now().timestamp_millis() as u64,
            order_type: order_type_enum,
            limit_type: limit_type_enum,
            status: Some(OrderStatus::New),
            block_number: event.block_number
        })
    } else {
        debug!("❌ Пропущены данные в событии: {:?}", event);
        None
    }
}
