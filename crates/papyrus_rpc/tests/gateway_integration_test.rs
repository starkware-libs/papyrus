use std::env;

use jsonrpsee::core::client::ClientT;
use jsonrpsee::http_client::{HttpClient, HttpClientBuilder};
use jsonrpsee::rpc_params;
use papyrus_common::transaction_hash::get_transaction_hash;
use papyrus_common::TransactionOptions;
use papyrus_rpc::{
    AddInvokeOkResultRPC0_4,
    InvokeTransactionRPC0_4,
    InvokeTransactionV1RPC0_4,
    TransactionVersion1RPC0_4,
};
use starknet_api::core::{ChainId, ContractAddress, EntryPointSelector, Nonce, PatriciaKey};
use starknet_api::transaction::{Calldata, Fee, Transaction, TransactionSignature};
use starknet_api::{calldata, contract_address, patricia_key};
use starknet_client::writer::objects::transaction::InvokeTransaction as SNClientInvokeTransaction;
use starknet_core::crypto::ecdsa_sign;
use starknet_core::types::FieldElement;
use starknet_types_core::felt::Felt;

const ETH_TO_WEI: u128 = u128::pow(10, 18);
const MAX_FEE: u128 = ETH_TO_WEI / 1000;
const INSUFFICIENT_FUNDS_STATUS_CODE: i32 = 2;
const L2_ETH_CONTRACT_ADDRESS: &str =
    "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7";
const BALANCE_OF_ENTRY_POINT_SELECTOR: &str =
    "0x2e4263afad30923c891518314c3c95dbe830a16874e8abc5777a9a20b54c76e";
const TRANSFER_ENTRY_POINT_SELECTOR: &str =
    "0x83afd3f4caedc6eebf44246fe54e38c95e3179a5ec9ea81740eca5b482d12e";
const USER_A_ADDRESS: &str = "0x2eda087f4edf190224eac3fdf7f762d83052f7c83fdda674e6e97e1f596a819";
const USER_B_ADDRESS: &str = "0x02d23bb72da2a2c7cce1577a013c3139b4f51d2b32be2ee7825f33428f572a9d";

// Returns the eth balance for the given account via the given node client.
async fn get_eth_balance(client: &HttpClient, account: ContractAddress) -> Felt {
    let balance = client
        .request::<Vec<Felt>, _>(
            "starknet_call",
            rpc_params!(
                (
                    L2_ETH_CONTRACT_ADDRESS,
                    EntryPointSelector(Felt::from_hex(BALANCE_OF_ENTRY_POINT_SELECTOR).unwrap()),
                    calldata![account.0.to_felt()],
                ),
                "latest"
            ),
        )
        .await
        .expect("Call to balanceOf failed.");
    balance[0]
}

#[tokio::test]
#[ignore]
// Sends a 'transfer of eth from user A to user B' transaction to a node instance synced with
// Starknet integration testnet. The node is expected to resend the transaction to Starknet
// successfully.
async fn test_gw_integration_testnet() {
    let node_url = env::var("INTEGRATION_TESTNET_NODE_URL")
        .expect("Node url must be given in INTEGRATION_TESTNET_NODE_URL environment variable.");
    let client =
        HttpClientBuilder::default().build(format!("https://{}:443/rpc/v0_4", node_url)).unwrap();
    let sender_address = contract_address!(Felt::from_hex(USER_A_ADDRESS).unwrap());
    // Sender balance sufficient balance should be maintained outside of this test.
    let sender_balance = get_eth_balance(&client, sender_address).await;
    if sender_balance <= MAX_FEE.into() {
        println!("Sender balance is too low. Please fund account {}.", USER_A_ADDRESS);
        std::process::exit(INSUFFICIENT_FUNDS_STATUS_CODE);
    }

    // TODO(tzahi): Switch to "pending" once supported and add an assertion for the status of the
    // sent tx and balance update in the end of this test.
    let nonce = client
        .request::<Nonce, _>("starknet_getNonce", rpc_params!["latest", sender_address])
        .await
        .unwrap();
    let receiver_address = contract_address!(Felt::from_hex(USER_B_ADDRESS).unwrap());

    // Create an invoke transaction for Eth transfer with a signature placeholder.
    let mut invoke_tx = InvokeTransactionV1RPC0_4 {
        max_fee: Fee(MAX_FEE),
        signature: TransactionSignature::default(),
        nonce,
        sender_address,
        version: TransactionVersion1RPC0_4::default(),
        calldata: calldata![
            Felt::ONE, // OpenZeppelin call array len (number of calls in this tx).
            // Call Array (4 elements per array struct element).
            Felt::from_hex(L2_ETH_CONTRACT_ADDRESS).unwrap(), // to
            EntryPointSelector(Felt::from_hex(TRANSFER_ENTRY_POINT_SELECTOR).unwrap()).0, /* selector. */
            Felt::ZERO, // data offset (in the calldata array)
            Felt::THREE, /* data len (of this call in the entire
                         * calldata array) */
            // Call data.
            Felt::THREE, // Call data len.
            // calldata for transfer - receiver and amount (uint256  = 2 felts).
            receiver_address.0.to_felt(),
            Felt::ONE, // LSB
            Felt::ZERO
        ],
    };

    // Update the signature.
    let hash = get_transaction_hash(
        &Transaction::Invoke(InvokeTransactionRPC0_4::Version1(invoke_tx.clone()).into()),
        &ChainId("SN_GOERLI".to_string()),
        &TransactionOptions::default(),
    )
    .unwrap();
    let signature = ecdsa_sign(
        &FieldElement::from_hex_be(&env::var("SENDER_PRIVATE_KEY").expect(
            "Sender private key must be given in SENDER_PRIVATE_KEY environment variable.",
        ))
        .unwrap(),
        &FieldElement::from_mont(hash.0.to_raw_reversed()),
    )
    .unwrap();
    invoke_tx.signature = TransactionSignature(vec![
        Felt::from_bytes_be(&signature.r.to_bytes_be()),
        Felt::from_bytes_be(&signature.s.to_bytes_be()),
    ]);

    let invoke_res = client
        .request::<AddInvokeOkResultRPC0_4, _>(
            "starknet_addInvokeTransaction",
            rpc_params!(SNClientInvokeTransaction::from(invoke_tx)),
        )
        .await
        .unwrap_or_else(|err| panic!("Failed to add tx '{hash}' with nonce '{nonce:?}'.: {err}"));

    println!("Invoke Tx result: {:?}", invoke_res);
}
