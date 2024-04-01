use starknet_api::core::ChainId;

pub(crate) mod serializers;
#[cfg(test)]
pub(crate) mod serializers_test;

pub(crate) mod compression_utils;
use tracing::warn;

use self::compression_utils::initialize_dictionary_compression_for_type;
use self::serializers::{
    SN_GOERLI_THIN_STATE_DIFF_DICTS,
    THIN_STATE_DIFF_DECODERS_DICTS_ARRAY,
    THIN_STATE_DIFF_DICT_VERSION,
    THIN_STATE_DIFF_ENCODER_DICT,
};
use crate::serialization::serializers::SN_MAIN_THIN_STATE_DIFF_DICTS;

// Initializes the variables needed for pre-trained dictionary compression for all the types.
pub(crate) fn initialize_dictionary_compression(chain_id: ChainId) {
    let thin_state_diff_dicts: &[&'static [u8]] = match chain_id.0.as_str() {
        "SN_MAIN" => &SN_MAIN_THIN_STATE_DIFF_DICTS,
        "SN_GOERLI" => &SN_GOERLI_THIN_STATE_DIFF_DICTS,
        _ => {
            warn!("Unrecognized chain id: {}, using empty compression dictionaries.", chain_id.0);
            &[&[]]
        }
    };

    initialize_dictionary_compression_for_type(
        thin_state_diff_dicts,
        &THIN_STATE_DIFF_DICT_VERSION,
        &THIN_STATE_DIFF_ENCODER_DICT,
        &THIN_STATE_DIFF_DECODERS_DICTS_ARRAY,
    );
}
