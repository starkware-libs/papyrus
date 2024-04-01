use std::sync::OnceLock;

use zstd::bulk::{Compressor, Decompressor};
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::db::serialization::{StorageSerde, StorageSerdeError};

#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

// TODO(dvir): move the old compression module to this directory.
// TODO(dvir): fine-tune the compression hyperparameters, especially compression level, magic_bytes
// and back reference distance.
// TODO(dvir): consider compressing the object only if it reduces the size by some threshold.

// An upper bound for the size of the decompressed data.
const MAX_DECOMPRESSION_CAPACITY: usize = 1 << 32; // 4GB

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub(crate) struct DictionaryVersion(pub u8);

// Compresses the given data using the given dictionary.
pub(crate) fn compress_with_pretrained_dictionary(
    bytes: &[u8],
    encoder_dict: &EncoderDictionary<'static>,
) -> Result<Vec<u8>, StorageSerdeError> {
    // TODO(dvir): create the compressor only once when initializing the dictionaries.
    // TODO(dvir): allocate a buffer once and use it for all the compressions.
    let mut compressor = Compressor::with_prepared_dictionary(encoder_dict)?;
    Ok(compressor.compress(bytes)?)
}

// Decompresses the given data using the given dictionary.
pub(crate) fn decompress_with_pretrained_dictionary(
    bytes: &[u8],
    decoder_dict: &DecoderDictionary<'static>,
) -> Result<Vec<u8>, StorageSerdeError> {
    // TODO(dvir): try (and consider) to create decompressor once for each thread.
    let mut decompressor = Decompressor::with_prepared_dictionary(decoder_dict)?;

    // TODO(dvir): try to create one buffer for each thread and decompress to it instead of
    // creating a new buffer each time.
    Ok(decompressor.decompress(bytes, MAX_DECOMPRESSION_CAPACITY)?)
}

// Compresses the given data using the given dictionary and writes the compressed data to the given
// writer.
pub(crate) fn compress_with_pretrained_dictionary_and_version(
    current_dict_version: &OnceLock<DictionaryVersion>,
    encoder: &OnceLock<EncoderDictionary<'static>>,
    bytes: &[u8],
    res: &mut impl std::io::Write,
) -> Result<(), StorageSerdeError> {
    current_dict_version.get().expect("Should be initialized.").serialize_into(res)?;
    let encoder = encoder.get().expect("Should be initialized.");
    let compressed = compress_with_pretrained_dictionary(bytes, encoder)?;
    compressed.serialize_into(res)?;
    Ok(())
}

// TODO(dvir): consider returning a Result with appropriate errors instead of Option.
// Reads a dictionary version, by it chooses a decoder from the decoder array and use it to
// decompress the data. The current_dict_version argument is only used to make sure we don't use an
// incorrect dictionary.
pub(crate) fn decompress_with_pretrained_dictionary_and_version<const DECODERS_NUM: usize>(
    current_dict_version: &OnceLock<DictionaryVersion>,
    decoders: &OnceLock<[DecoderDictionary<'static>; DECODERS_NUM]>,
    bytes: &mut impl std::io::Read,
) -> Option<Vec<u8>> {
    let version = DictionaryVersion::deserialize_from(bytes)?;
    // TOOD(dvir): consider remove this check.
    if &version > current_dict_version.get().expect("Should be initialized.") {
        panic!("The version of the dictionary is higher than the current version.");
    }

    let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
    let decoder_dict = &decoders.get().expect("Should be initialized.")[version.0 as usize];
    let decompressed =
        decompress_with_pretrained_dictionary(compressed_data.as_slice(), decoder_dict).ok()?;
    Some(decompressed)
}

// Initializing all the information needed for dictionary compression of some type.
pub(crate) fn initialize_dictionary_compression_for_type<const DECODERS_NUM: usize>(
    dicts: &[&'static [u8]],
    dict_version_target: &OnceLock<DictionaryVersion>,
    encoder_target: &OnceLock<EncoderDictionary<'static>>,
    decoders_target: &OnceLock<[DecoderDictionary<'static>; DECODERS_NUM]>,
) {
    let number_of_dicts = dicts.len();
    let _ = dict_version_target.set(DictionaryVersion((number_of_dicts - 1) as u8));
    let _ = encoder_target
        .set(EncoderDictionary::copy(dicts[number_of_dicts - 1], zstd::DEFAULT_COMPRESSION_LEVEL));

    let mut dict_array: [DecoderDictionary<'static>; DECODERS_NUM] =
        array_macro::array![_ => DecoderDictionary::copy(&[]); DECODERS_NUM];
    for (idx, dict) in dicts.iter().enumerate() {
        dict_array[idx] = DecoderDictionary::copy(dict);
    }
    let _ = decoders_target.set(dict_array);
}
