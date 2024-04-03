use std::sync::OnceLock;

use zstd::dict::{DDict, DecoderDictionary, EncoderDictionary};

use crate::serialization::compression_utils::{
    compress_with_pretrained_dictionary,
    compress_with_pretrained_dictionary_and_version,
    decompress_with_pretrained_dictionary,
    decompress_with_pretrained_dictionary_and_version,
    initialize_dictionary_compression_for_type,
    DictionaryVersion,
};

const DATA_BYTES: &[u8] = b"hello world";

const TEST_DICTIONARIES: [&[u8]; 2] = [DICT_0, DICT_1];
static TEST_DICT_VERSION: OnceLock<DictionaryVersion> = OnceLock::new();
static TEST_ENCODER: OnceLock<EncoderDictionary<'static>> = OnceLock::new();
static TEST_DECODERS: OnceLock<[DecoderDictionary<'static>; 2]> = OnceLock::new();

// Real trained dictionaries.
const DICT_0: &[u8] = &[
    55, 164, 48, 236, 70, 152, 42, 87, 38, 16, 128, 190, 27, 252, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 127, 6, 152, 249, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 223, 211, 14, 227, 76, 8, 33, 132, 16, 66, 136, 136, 136, 60, 84, 160,
    64, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65,
    65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 193, 245, 82, 74, 169, 170,
    234, 1, 100, 160, 170, 193, 96, 48, 24, 12, 6, 131, 193, 96, 48, 12, 195, 48, 12, 195, 48, 12,
    195, 48, 198, 24, 99, 140, 153, 29, 1, 0, 0, 0, 4, 0, 0, 0, 8, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1,
];

const DICT_1: &[u8] = &[
    55, 164, 48, 236, 116, 171, 151, 38, 38, 16, 128, 190, 27, 252, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 255, 255, 255, 127, 6, 152, 249, 255, 255, 255, 255, 255, 255, 255,
    255, 255, 255, 255, 255, 239, 55, 11, 227, 76, 8, 33, 132, 16, 66, 136, 136, 136, 60, 84, 160,
    64, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65,
    65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 65, 193, 245, 82, 74, 169, 170,
    234, 1, 100, 160, 170, 193, 96, 48, 24, 12, 6, 131, 193, 96, 48, 12, 195, 48, 12, 195, 48, 12,
    195, 48, 198, 24, 99, 140, 153, 29, 1, 0, 0, 0, 4, 0, 0, 0, 8, 0, 0, 0, 2, 2, 2, 2, 2, 2, 2, 2,
];

#[test]
fn compress_decompress_with_pretrained_dictionary() {
    let encoder_dict = EncoderDictionary::copy(DICT_0, zstd::DEFAULT_COMPRESSION_LEVEL);
    let decoder_dict = DecoderDictionary::copy(DICT_0);

    let compressed = compress_with_pretrained_dictionary(DATA_BYTES, &encoder_dict).unwrap();
    let decompressed = decompress_with_pretrained_dictionary(&compressed, &decoder_dict).unwrap();

    assert_eq!(DATA_BYTES, decompressed.as_slice());
}

#[test]
fn decompression_fails_with_wrong_dictionary() {
    let encoder_dict = EncoderDictionary::copy(DICT_0, zstd::DEFAULT_COMPRESSION_LEVEL);
    let compressed = compress_with_pretrained_dictionary(DATA_BYTES, &encoder_dict).unwrap();

    let decoder_dict = DecoderDictionary::copy(DICT_1);

    assert!(decompress_with_pretrained_dictionary(&compressed, &decoder_dict).is_err());
}

#[test]
fn compress_decompress_with_pretrained_dictionary_and_version() {
    initialize_dictionary_compression_for_type(
        &TEST_DICTIONARIES,
        &TEST_DICT_VERSION,
        &TEST_ENCODER,
        &TEST_DECODERS,
    );

    let mut buffer = Vec::new();
    compress_with_pretrained_dictionary_and_version(
        &TEST_DICT_VERSION,
        &TEST_ENCODER,
        DATA_BYTES,
        &mut buffer,
    )
    .unwrap();

    let decompressed = decompress_with_pretrained_dictionary_and_version(
        &TEST_DICT_VERSION,
        &TEST_DECODERS,
        &mut buffer.as_slice(),
    )
    .unwrap();

    assert_eq!(DATA_BYTES, decompressed.as_slice());
}

#[test]
#[should_panic(expected = "The version of the dictionary is higher than the current version.")]
fn decompression_fails_with_higher_version() {
    initialize_dictionary_compression_for_type(
        &TEST_DICTIONARIES,
        &TEST_DICT_VERSION,
        &TEST_ENCODER,
        &TEST_DECODERS,
    );

    let high_version = OnceLock::new();
    high_version.set(DictionaryVersion(u8::MAX)).unwrap();
    let mut buffer = Vec::new();

    compress_with_pretrained_dictionary_and_version(
        &high_version,
        &TEST_ENCODER,
        DATA_BYTES,
        &mut buffer,
    )
    .unwrap();

    assert!(
        decompress_with_pretrained_dictionary_and_version(
            &TEST_DICT_VERSION,
            &TEST_DECODERS,
            &mut buffer.as_slice(),
        )
        .is_none()
    );
}

#[test]
fn decompression_for_old_dicts() {
    initialize_dictionary_compression_for_type(
        &TEST_DICTIONARIES,
        &TEST_DICT_VERSION,
        &TEST_ENCODER,
        &TEST_DECODERS,
    );

    for (idx, dict) in TEST_DICTIONARIES.iter().enumerate() {
        let version = OnceLock::new();
        version.set(DictionaryVersion(idx as u8)).unwrap();

        let encoder = OnceLock::new();
        encoder.set(EncoderDictionary::copy(dict, zstd::DEFAULT_COMPRESSION_LEVEL)).unwrap_or_else(
            |_| {
                panic!("Failed to set encoder for dictionary {}", idx);
            },
        );

        let mut buffer = Vec::new();
        compress_with_pretrained_dictionary_and_version(
            &version,
            &encoder,
            DATA_BYTES,
            &mut buffer,
        )
        .unwrap();

        let decompressed = decompress_with_pretrained_dictionary_and_version(
            &TEST_DICT_VERSION,
            &TEST_DECODERS,
            &mut buffer.as_slice(),
        )
        .unwrap();
        assert_eq!(DATA_BYTES, decompressed.as_slice());
    }
}

#[test]
fn init_compression_dict_test() {
    const DICTS: &[&[u8]] = &[DICT_0, DICT_1];
    let dict_version_target = OnceLock::new();
    let encoder_target = OnceLock::new();
    let decoders_target: OnceLock<[DecoderDictionary<'static>; 3]> = OnceLock::new();

    initialize_dictionary_compression_for_type(
        DICTS,
        &dict_version_target,
        &encoder_target,
        &decoders_target,
    );

    let expected_dict_version = DictionaryVersion((DICTS.len() - 1) as u8);
    assert_eq!(*dict_version_target.get().unwrap(), expected_dict_version);

    let expected_encoder = EncoderDictionary::copy(DICT_1, zstd::DEFAULT_COMPRESSION_LEVEL);
    assert_eq!(
        encoder_target.get().unwrap().as_cdict().get_dict_id().unwrap(),
        expected_encoder.as_cdict().get_dict_id().unwrap()
    );

    assert_eq!(
        DDict::create(DICT_0).get_dict_id().unwrap(),
        decoders_target.get().unwrap()[0].as_ddict().get_dict_id().unwrap()
    );
    assert_eq!(
        DDict::create(DICT_1).get_dict_id().unwrap(),
        decoders_target.get().unwrap()[1].as_ddict().get_dict_id().unwrap()
    );
    assert_eq!(
        DDict::create(&[]).get_dict_id(),
        decoders_target.get().unwrap()[2].as_ddict().get_dict_id()
    );
}
