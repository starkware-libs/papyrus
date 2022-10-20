use rand::Rng;

use super::common_proto::{BlockHeader, FieldElement};

#[test]
fn common_block_header_roundtrip() -> Result<(), anyhow::Error> {
    let _asd = BlockHeader {
        parent_block_hash: Some(FieldElement {
            element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
        }),
        block_number: rand::thread_rng().gen::<u64>(),
        global_state_root: Some(FieldElement {
            element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
        }),
        sequencer_address: Some(FieldElement {
            element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
        }),
        block_timestamp: rand::thread_rng().gen::<u64>(),
        // transaction_count: rand::thread_rng().gen::<u32>(),
        // transaction_commitment: Some(FieldElement {
        //     element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
        // }),

        // event_count: rand::thread_rng().gen::<u32>(),
        // event_commitment: Some(FieldElement {
        //     element: rand::thread_rng().gen::<[u8; 32]>().to_vec(),
        // }),

        // protocol_version: rand::thread_rng().gen::<u32>(),
    };

    Ok(())
}
