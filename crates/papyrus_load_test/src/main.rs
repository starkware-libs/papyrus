// This code is inspired by the pathfinder load test.
// To run this load test, run locally a node and then run:
//      cargo run -r -p papyrus_load_test -- -t 5m -H http://127.0.0.1:8080
// For more options run:
//      cargo run -r -p papyrus_load_test -- --help

use std::env;
use std::fs::File;

use goose::goose::{Scenario, Transaction};
use goose::{scenario, transaction, util, GooseAttack};
use papyrus_load_test::load_tests::*;
use serde::Serialize;

fn register_scenarios(goose: GooseAttack) -> GooseAttack {
    goose
        .register_scenario(
            scenario!("block_number").register_transaction(transaction!(loadtest_block_number)),
        )
        .register_scenario(
            scenario!("block_hash_and_number")
                .register_transaction(transaction!(loadtest_block_hash_and_number)),
        )
        .register_scenario(
            scenario!("get_block_with_tx_hashes_by_number")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_block_with_tx_hashes_by_hash")
                .register_transaction(transaction!(loadtest_get_block_with_tx_hashes_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_block_with_full_transactions_by_number").register_transaction(
                transaction!(loadtest_get_block_with_full_transactions_by_number),
            ),
        )
        .register_scenario(
            scenario!("loadtest_get_block_with_full_transactions_by_hash").register_transaction(
                transaction!(loadtest_get_block_with_full_transactions_by_hash),
            ),
        )
        .register_scenario(
            scenario!("loadtest_get_storage_at_by_number")
                .register_transaction(transaction!(loadtest_get_storage_at_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_storage_at_by_hash")
                .register_transaction(transaction!(loadtest_get_storage_at_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_transaction_by_hash")
                .register_transaction(transaction!(loadtest_get_transaction_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_transaction_by_block_id_and_index_by_number")
                .register_transaction(transaction!(
                    loadtest_get_transaction_by_block_id_and_index_by_number
                )),
        )
        .register_scenario(
            scenario!("loadtest_get_transaction_by_block_id_and_index_by_hash")
                .register_transaction(transaction!(
                    loadtest_get_transaction_by_block_id_and_index_by_hash
                )),
        )
        .register_scenario(
            scenario!("loadtest_get_block_transaction_count_by_number")
                .register_transaction(transaction!(loadtest_get_block_transaction_count_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_block_transaction_count_by_hash")
                .register_transaction(transaction!(loadtest_get_block_transaction_count_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_state_update_by_number")
                .register_transaction(transaction!(loadtest_get_state_update_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_state_update_by_hash")
                .register_transaction(transaction!(loadtest_get_state_update_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_transaction_receipt")
                .register_transaction(transaction!(loadtest_get_transaction_receipt)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_by_number")
                .register_transaction(transaction!(loadtest_get_class_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_by_hash")
                .register_transaction(transaction!(loadtest_get_class_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_at_by_number")
                .register_transaction(transaction!(loadtest_get_class_at_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_at_by_hash")
                .register_transaction(transaction!(loadtest_get_class_at_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_hash_at_by_number")
                .register_transaction(transaction!(loadtest_get_class_hash_at_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_class_hash_at_by_hash")
                .register_transaction(transaction!(loadtest_get_class_hash_at_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_get_nonce_by_number")
                .register_transaction(transaction!(loadtest_get_nonce_by_number)),
        )
        .register_scenario(
            scenario!("loadtest_get_nonce_by_hash")
                .register_transaction(transaction!(loadtest_get_nonce_by_hash)),
        )
        .register_scenario(
            scenario!("loadtest_chain_id").register_transaction(transaction!(loadtest_chain_id)),
        )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let goose = register_scenarios(GooseAttack::initialize()?);
    let metrics = goose.execute().await?;

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
