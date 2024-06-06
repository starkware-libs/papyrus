#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

// Compress the value using gzip with the default compression level and encode it in base64.
pub fn compress_and_encode(value: serde_json::Value) -> Result<String, std::io::Error> {
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    serde_json::to_writer(&mut compressor, &value)?;
    let compressed_data = compressor.finish()?;
    Ok(base64::encode(compressed_data))
}
