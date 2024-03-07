mod common;
mod header;

#[derive(thiserror::Error, Debug)]
pub enum ProtobufConversionError {
    #[error("Type `{type_description}` got out of range value {value_as_str}")]
    OutOfRangeValue { type_description: &'static str, value_as_str: String },
    #[error("Missing field `{field_description}`")]
    MissingField { field_description: &'static str },
    #[error("Type `{type_description}` should be {num_expected} bytes but it got {value:?}.")]
    BytesDataLengthMismatch { type_description: &'static str, num_expected: usize, value: Vec<u8> },
}

#[derive(thiserror::Error, Debug)]
pub enum ProtobufBlockHeaderResponseToDataError {
    #[error("Type `{type_description}` got unsupported data type {data_type}")]
    UnsupportedDataType { data_type: String, type_description: String },
}
