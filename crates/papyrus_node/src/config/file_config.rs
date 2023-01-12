use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use std::{env, fs};

use papyrus_gateway::GatewayConfig;
use papyrus_monitoring_gateway::MonitoringGatewayConfig;
use papyrus_storage::{DbConfig, StorageConfig};
use papyrus_sync::{CentralSourceConfig, SyncConfig};
use serde::{Deserialize, Serialize};
use starknet_api::core::ChainId;
use starknet_client::RetryConfig;

use crate::config::ConfigBuilder;

// Defines the expected structure of the configuration file. All the fields are optional so the user
// doesn't have to specify parameters that he doesn't wish to override (in that case the previous
// value remains).
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileConfigFormat {
    chain_id: Option<ChainId>,
    central: Option<Central>,
    gateway: Option<Gateway>,
    monitoring_gateway: Option<MonitoringGateway>,
    storage: Option<Storage>,
    sync: Option<Sync>,
}

impl FileConfigFormat {
    // Apply the configuration as given by the file on the configuration builder.
    pub(crate) fn update_config(self, builder: &mut ConfigBuilder) {
        if let Some(chain_id) = self.chain_id {
            builder.chain_id = chain_id;
        }

        if let Some(central) = self.central {
            central.update_central(&mut builder.config.central);
        }

        if let Some(gateway) = self.gateway {
            gateway.update_gateway(&mut builder.config.gateway);
        }

        if let Some(monitoring_gateway) = self.monitoring_gateway {
            monitoring_gateway.update_monitoring_gateway(&mut builder.config.monitoring_gateway);
        }

        if let Some(storage) = self.storage {
            storage.update_storage(&mut builder.config.storage);
        }

        // Sync is optional, override it only if it is Some in the builder.
        if let (Some(builder_config), Some(file_config)) = (builder.config.sync.as_mut(), self.sync)
        {
            file_config.update_sync(builder_config)
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Central {
    concurrent_requests: Option<usize>,
    url: Option<String>,
    http_headers: Option<HashMap<String, String>>,
    retry: Option<Retry>,
}

impl Central {
    fn update_central(self, config: &mut CentralSourceConfig) {
        if let Some(concurrent_requests) = self.concurrent_requests {
            config.concurrent_requests = concurrent_requests;
        }
        if let Some(url) = self.url {
            config.url = url;
        }
        if let Some(http_headers) = self.http_headers {
            config.http_headers.extend(http_headers);
        }
        if let Some(retry) = self.retry {
            retry.update_retry_config(&mut config.retry_config);
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Retry {
    retry_base_millis: Option<u64>,
    retry_max_delay_millis: Option<u64>,
    max_retries: Option<usize>,
}

impl Retry {
    fn update_retry_config(self, config: &mut RetryConfig) {
        if let Some(retry_base_millis) = self.retry_base_millis {
            config.retry_base_millis = retry_base_millis;
        }
        if let Some(retry_max_delay_millis) = self.retry_max_delay_millis {
            config.retry_max_delay_millis = retry_max_delay_millis;
        }
        if let Some(max_retries) = self.max_retries {
            config.max_retries = max_retries;
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Gateway {
    server_address: Option<String>,
    max_events_chunk_size: Option<usize>,
    max_events_keys: Option<usize>,
}

impl Gateway {
    fn update_gateway(self, config: &mut GatewayConfig) {
        if let Some(server_address) = self.server_address {
            config.server_address = server_address;
        }
        if let Some(max_events_chunk_size) = self.max_events_chunk_size {
            config.max_events_chunk_size = max_events_chunk_size;
        }
        if let Some(max_events_keys) = self.max_events_keys {
            config.max_events_keys = max_events_keys;
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct MonitoringGateway {
    server_address: Option<String>,
}

impl MonitoringGateway {
    fn update_monitoring_gateway(self, config: &mut MonitoringGatewayConfig) {
        if let Some(server_address) = self.server_address {
            config.server_address = server_address;
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Storage {
    db: Option<Db>,
}

impl Storage {
    fn update_storage(self, config: &mut StorageConfig) {
        if let Some(db) = self.db {
            db.update_db(&mut config.db_config);
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Db {
    path: Option<String>,
    max_size: Option<usize>,
}

impl Db {
    fn update_db(self, config: &mut DbConfig) {
        if let Some(path) = self.path {
            config.path = path;
        }
        if let Some(max_size) = self.max_size {
            config.max_size = max_size;
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Sync {
    block_propagation_sleep_duration: Option<Duration>,
    recoverable_error_sleep_duration: Option<Duration>,
}

impl Sync {
    fn update_sync(self, config: &mut SyncConfig) {
        if let Some(block_propagation_sleep_duration) = self.block_propagation_sleep_duration {
            config.block_propagation_sleep_duration = block_propagation_sleep_duration;
        }
        if let Some(recoverable_error_sleep_duration) = self.recoverable_error_sleep_duration {
            config.recoverable_error_sleep_duration = recoverable_error_sleep_duration;
        }
    }
}
