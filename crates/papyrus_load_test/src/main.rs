// This code is inspired by the pathfinder load test.
// To run this load test, run locally a node and then run:
//      cargo run -r -p papyrus_load_test -- -t 5m -H http://127.0.0.1:8080
// For more options run:
//      cargo run -r -p papyrus_load_test -- --help

use std::env;
use std::fs::File;

use goose::goose::{GooseUser, Scenario, Transaction, TransactionResult};
use goose::{scenario, transaction, util, GooseAttack};
use papyrus_load_test::gateway_functions::{
    get_block_w_tx_hashes_by_hash, get_block_w_tx_hashes_by_number,
};
use serde::Serialize;

async fn loadtest_get_block_with_tx_hashes_by_number(user: &mut GooseUser) -> TransactionResult {
    let _: serde_json::Value = get_block_w_tx_hashes_by_number(user, 1).await?;
    Ok(())
}

async fn loadtest_get_block_with_tx_hashes_by_hash(user: &mut GooseUser) -> TransactionResult {
    // TODO(shahak): Get a hash by getting a block instead of relying on that this hash exists.
    let _: serde_json::Value = get_block_w_tx_hashes_by_hash(
        user,
        "0x1d997fd79d81bb4c30c78d7cb32fb8a59112eeb86347446235cead6194aed07",
    )
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let metrics = GooseAttack::initialize()?
        .register_scenario(
            scenario!("block_by_number")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_number)),
        )
        .register_scenario(
            scenario!("block_by_hash")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_hash)),
        )
        .execute()
        .await?;

    // The OUTPUT_FILE env is expected to be a valid path in the os.
    // If exists, aggregated results will be written to that path in the following json format:
    // [
    //     {
    //         "name": <scenario name>,
    //         "units": "Milliseconds",
    //         "value": <scenario median time>,
    //     },
    // ]
    if let Ok(path) = env::var("OUTPUT_FILE") {
        let file = File::create(path)?;
        let mut data: Vec<Entry> = vec![];
        for scenario in metrics.scenarios {
            let median = util::median(
                &scenario.times,
                scenario.counter,
                scenario.min_time,
                scenario.max_time,
            );
            data.push(Entry {
                name: scenario.name,
                units: "Milliseconds".to_string(),
                value: median,
            });
        }
        serde_json::to_writer(file, &data)?
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Entry {
    name: String,
    units: String, // "Milliseconds"
    value: usize,
}
