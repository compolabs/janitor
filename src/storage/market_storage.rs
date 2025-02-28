use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};
use serde::Deserialize;
use std::fs;
use tracing::info;

use crate::error::Error;

use crate::indexer::spot_order::SpotOrder;

#[derive(Debug, Deserialize, Clone)]
pub struct TradingPairConfig {
    pub symbol: String,
    pub contract_id: String,
    pub start_block: i64,
    pub description: String,
    pub decimals: i32,
}

#[derive(Debug)]
pub struct OrderQueue {
    
    pub orders: RwLock<VecDeque<SpotOrder>>,
    pub cancelling_orders: RwLock<HashSet<String>>,
}

impl OrderQueue {
    pub fn new() -> Self {
        Self {
            orders: RwLock::new(VecDeque::new()),
            cancelling_orders: RwLock::new(HashSet::new()),
        }
    }

    
    pub fn add_order(&self, order: SpotOrder) {
        let mut queue = self.orders.write().unwrap();
        queue.push_back(order);
    }

    
    pub fn remove_order(&self, order_id: &str) {
        let mut queue = self.orders.write().unwrap();
        queue.retain(|o| o.id != order_id);

        let mut cancelling = self.cancelling_orders.write().unwrap();
        cancelling.remove(order_id);
    }

    pub fn mark_as_cancelling(&self, order_id: &str) -> bool {
        let mut cancelling = self.cancelling_orders.write().unwrap();
        if cancelling.contains(order_id) {
            return false; 
        }
        cancelling.insert(order_id.to_string());
        true
    }

    pub fn is_cancelling(&self, order_id: &str) -> bool {
        let cancelling = self.cancelling_orders.read().unwrap();
        cancelling.contains(order_id)
    }

    
    pub fn get_all_orders(&self) -> Vec<SpotOrder> {
        let queue = self.orders.read().unwrap();
        queue.iter().cloned().collect()
    }

    pub fn update_order(&self, updated_order: SpotOrder) {
        let mut queue = self.orders.write().unwrap();
        if let Some(pos) = queue.iter().position(|o| o.id == updated_order.id) {
            queue[pos] = updated_order;
        }
    }

    
    pub fn pop_order(&self) -> Option<SpotOrder> {
        let mut queue = self.orders.write().unwrap();
        queue.pop_front()
    }
}

impl Default for OrderQueue {
    fn default() -> Self {
        Self::new()
    }
}




pub struct TradingEngine {
    pub markets: HashMap<String, Arc<OrderQueue>>,
    pub configs: HashMap<String, TradingPairConfig>,
}

impl TradingEngine {
    
    pub fn new(configs: Vec<TradingPairConfig>) -> Self {
        let markets = configs
            .iter()
            .map(|pair| (pair.symbol.clone(), Arc::new(OrderQueue::new())))
            .collect();
        let configs = configs
            .into_iter()
            .map(|pair| (pair.symbol.clone(), pair))
            .collect();
        Self { markets, configs }
    }

    
    pub fn load_config(path: &str) -> Result<Vec<TradingPairConfig>, Error> {
        let config_data = fs::read_to_string(path)?;
        let config: Vec<TradingPairConfig> = serde_json::from_str(&config_data)?;
        Ok(config)
    }

    
    pub fn get_market_queue(&self, symbol: &str) -> Option<Arc<OrderQueue>> {
        self.markets.get(symbol).cloned()
    }

    
    pub fn print_markets(&self) {
        for (symbol, config) in &self.configs {
            info!(
                "Loaded market: {} (contract_id: {}, start_block: {})",
                symbol, config.contract_id, config.start_block
            );
        }
    }
}
