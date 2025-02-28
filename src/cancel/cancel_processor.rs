use std::env;
use std::str::FromStr;
use std::sync::Arc;

use fuels::accounts::{provider::Provider, wallet::WalletUnlocked};
use fuels::types::{Bits256, ContractId};
use spark_market_sdk::SparkMarketContract;
use tracing::{error, info};

use crate::indexer::spot_order::SpotOrder;


#[derive(Clone, Default)]
pub struct CancelProcessor;

impl CancelProcessor {
    
    pub fn new() -> Self {
        Self
    }

    
    pub async fn get_current_block(&self) -> Result<i64, String> {
        let rpc_url =  "mainnet.fuel.network".to_string();
        let provider = Provider::connect(&rpc_url).await
            .map_err(|e| format!("Failed to connect provider: {:?}", e))?;

        provider.latest_block_height().await
            .map(|b| b as i64)
            .map_err(|e| format!("Failed to get latest block: {:?}", e))
    }

    
    pub async fn cancel_order(&self, order: SpotOrder) -> Result<(), String> {
        info!("⏳ Cancelling order: {}", order.id);

        
        let rpc_url =  "mainnet.fuel.network".to_string();
        let contract_id_str = env::var("CONTRACT_ID").map_err(|_| "Missing CONTRACT_ID in .env".to_string())?;
        let mnemonic = env::var("MNEMONIC").map_err(|_| "Missing MNEMONIC in .env".to_string())?;

        
        let provider = Provider::connect(&rpc_url)
            .await
            .map_err(|e| format!("Failed to connect provider: {:?}", e))?;

        
        let wallet = self.create_wallet(provider, &mnemonic).await?;

        
        let contract_id = ContractId::from_str(&contract_id_str)
            .map_err(|e| format!("Invalid contract ID: {:?}", e))?;
        let contract = SparkMarketContract::new(contract_id, wallet).await;

        
        let order_id_bits = Bits256::from_hex_str(&order.id)
            .map_err(|e| format!("Invalid order id hex: {:?}", e))?;

        match contract.cancel_order(order_id_bits).await {
            Ok(call_result) => {
                info!("✅ Cancelled order {} in tx: {:?}", order.id, call_result.tx_id);
                Ok(())
            }
            Err(e) => {
                error!("❌ Cancel transaction failed: {:?}", e);
                Err(format!("Cancel failed: {:?}", e))
            }
        }
    }

    
    async fn create_wallet(&self, provider: Provider, mnemonic: &str) -> Result<WalletUnlocked, String> {
        let wallet = WalletUnlocked::new_from_mnemonic_phrase(
            mnemonic,
            Some(provider),
        )
        .map_err(|e| format!("Failed to create wallet: {:?}", e))?;

        Ok(wallet)
    }
}
