use async_trait::async_trait;
use fuels::accounts::provider::Provider;
use fuels::types::Address;
use pangea_client::{
    futures::StreamExt, provider::FuelProvider, query::Bound, requests::fuel::GetSparkOrderRequest,
    ClientBuilder, Format, WsProvider,
};
use pangea_client::{ChainId, Client};
use std::collections::HashSet;
use tokio::task::JoinHandle;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{error, info};

use crate::config::ev;
use crate::error::Error;
use crate::indexer::pangea_event_handler::{handle_order_event, PangeaOrderEvent};
use crate::storage::market_storage::{TradingEngine, TradingPairConfig};
use super::Indexer;

pub struct PangeaIndexer {
    trading_engine: Arc<TradingEngine>,
}

impl PangeaIndexer {
    pub fn new(trading_engine: Arc<TradingEngine>) -> Self {
        Self { trading_engine }
    }
}

#[async_trait]
impl Indexer for PangeaIndexer {
    async fn initialize(&self, tasks: &mut Vec<JoinHandle<()>>) -> Result<(), Error> {
        let configs = self.trading_engine.configs.values().cloned().collect::<Vec<_>>();

        for config in configs {
            let trading_engine = Arc::clone(&self.trading_engine);
            let config_symbol = config.symbol.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = process_market_orders(trading_engine, config).await {
                    error!("Error processing market {}: {}", config_symbol, e);
                }
            }));
        }

        Ok(())
    }
}

async fn process_market_orders(
    trading_engine: Arc<TradingEngine>,
    config: TradingPairConfig,
) -> Result<(), Error> {
    let client = create_pangea_client().await?;
    let contract_h256 = Address::from_str(&config.contract_id).unwrap();

    let last_processed_block =
        fetch_historical_data(&client, &trading_engine, config.start_block, contract_h256, config.symbol.clone()).await?;

    info!(
        "✅ Sync complete for {}. Switching to real-time indexing from block {}",
        config.symbol, last_processed_block
    );

    listen_for_new_deltas(trading_engine, last_processed_block, contract_h256, config.symbol).await
}

async fn create_pangea_client() -> Result<Client<WsProvider>, Error> {
    info!("create_pangea_client");
    let username = ev("PANGEA_USERNAME")?;
    info!("username {:?}", username);
    let password = ev("PANGEA_PASSWORD")?;
    let url = ev("PANGEA_URL")?;

    let client = ClientBuilder::default()
        .endpoint(&url)
        .credential(username, password)
        .build::<WsProvider>()
        .await?;

    info!("✅ Connected to Pangea WebSocket at {}", url);
    Ok(client)
}

async fn fetch_historical_data(
    client: &Client<WsProvider>,
    trading_engine: &Arc<TradingEngine>,
    start_block: i64,
    contract_h256: Address,
    symbol: String,
) -> Result<i64, Error> {
    let fuel_chain = ChainId::FUEL;
    let latest_block = get_latest_block().await?;

    info!(
        "📌 Syncing historical data for {} from block {} to {}...",
        symbol, start_block, latest_block
    );

    let request = GetSparkOrderRequest {
        from_block: Bound::Exact(start_block),
        to_block: Bound::Exact(latest_block),
        market_id__in: HashSet::from([contract_h256]),
        chains: HashSet::from([fuel_chain]),
        ..Default::default()
    };

    let stream = client.get_fuel_spark_orders_by_format(request, Format::JsonStream, false).await?;
    pangea_client::futures::pin_mut!(stream);

    while let Some(data) = stream.next().await {
        if let Ok(data) = data {
            if let Ok(order_event) = serde_json::from_slice::<PangeaOrderEvent>(&data) {
                handle_order_event(trading_engine.clone(), order_event, symbol.clone()).await;
            } else {
                error!("Failed to deserialize order event");
            }
        }
    }

    Ok(latest_block)
}

async fn listen_for_new_deltas(
    trading_engine: Arc<TradingEngine>,
    mut last_processed_block: i64,
    contract_h256: Address,
    symbol: String,
) -> Result<(), Error> {
    let mut retry_delay = Duration::from_secs(1);
    let max_backoff = Duration::from_secs(60);

    loop {
        let client = match create_pangea_client().await {
            Ok(c) => {
                retry_delay = Duration::from_secs(1);
                c
            }
            Err(e) => {
                error!("Failed to create Pangea client: {}", e);
                sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(max_backoff);
                continue;
            }
        };

        let request = GetSparkOrderRequest {
            from_block: Bound::Exact(last_processed_block + 1),
            to_block: Bound::Subscribe,
            market_id__in: HashSet::from([contract_h256]),
            chains: HashSet::from([ChainId::FUEL]),
            ..Default::default()
        };

        match timeout(
            Duration::from_secs(10),
            client.get_fuel_spark_orders_by_format(request, Format::JsonStream, true),
        )
        .await
        {
            Ok(Ok(mut stream)) => {
                info!("Subscribed to real-time deltas from block {}", last_processed_block + 1);
                
                retry_delay = Duration::from_secs(1);

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(raw_bytes) => {
                            match serde_json::from_slice::<PangeaOrderEvent>(&raw_bytes) {
                                Ok(order_event) => {
                                    last_processed_block = order_event.block_number;
                                    handle_order_event(
                                        trading_engine.clone(),
                                        order_event,
                                        symbol.clone()
                                    ).await;
                                }
                                Err(e) => {
                                    error!("Failed to deserialize order event: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Stream error: {}", e);
                            break;
                        }
                    }
                }
            }
            _ => {
                error!("Failed to subscribe to new deltas, retrying...");
            }
        }
        sleep(retry_delay).await;
        retry_delay = (retry_delay * 2).min(max_backoff);
    }
}

async fn get_latest_block() -> Result<i64, Error> {
    let provider_url = "mainnet.fuel.network";
    let provider = Provider::connect(provider_url).await?;
    Ok(provider.latest_block_height().await? as i64)
}
