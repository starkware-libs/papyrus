// TODO(shahak): Internalize this once network doesn't depend on protobuf.
pub mod common;
mod header;
mod receipt;
// TODO(shahak): Internalize this once network doesn't depend on protobuf.
pub mod state_diff;
mod transaction;

#[derive(thiserror::Error, Debug)]
pub enum ProtobufConversionError {
    #[error("Type `{type_description}` got out of range value {value_as_str}")]
    OutOfRangeValue { type_description: &'static str, value_as_str: String },
    #[error("Missing field `{field_description}`")]
    MissingField { field_description: &'static str },
    #[error("Type `{type_description}` should be {num_expected} bytes but it got {value:?}.")]
    BytesDataLengthMismatch { type_description: &'static str, num_expected: usize, value: Vec<u8> },
}
