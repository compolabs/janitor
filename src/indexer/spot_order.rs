use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// NTD Adapt spark-sdk OrderType to that type
#[derive(Debug, PartialEq, Eq, Clone, Copy, JsonSchema, Serialize, Deserialize)]
pub enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, JsonSchema, Serialize, Deserialize)]
pub enum LimitType {
    FOK,
    IOC,
    GTC,
    MKT,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, JsonSchema, Serialize, Deserialize)]
pub enum OrderStatus {
    New,
    PartiallyMatched,
    Matched,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, JsonSchema, Serialize, Deserialize, Eq)]
pub struct SpotOrder {
    pub id: String,
    pub user: String,
    pub asset: String,
    pub amount: u128,
    pub price: u128,
    pub timestamp: u64,
    pub order_type: OrderType,
    pub limit_type: Option<LimitType>,
    pub status: Option<OrderStatus>,
    pub block_number: i64, 
}

impl PartialEq for SpotOrder {
    fn eq(&self, other: &Self) -> bool {
        self.price == other.price
    }
}

impl Ord for SpotOrder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.price
            .cmp(&other.price)
            .then_with(|| self.timestamp.cmp(&other.timestamp))
    }
}

impl PartialOrd for SpotOrder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
