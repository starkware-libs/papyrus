use std::io::{Result as IOResult, Write};

use serde::Serialize;
use serde_json::ser::{CompactFormatter, Formatter};

// TODO: Consider moving to SN API as a method of deprecated_contract_class::ContractClass.
pub fn calculate_deprecated_class_abi_length(
    deprecated_class: &starknet_api::deprecated_contract_class::ContractClass,
) -> Result<usize, serde_json::Error> {
    let Some(abi) = deprecated_class.abi.as_ref() else {
        return Ok(0);
    };
    let mut chars = vec![];
    abi.serialize(&mut serde_json::Serializer::with_formatter(&mut chars, PythonJsonFormatter))?;
    Ok(chars.len())
}

/// Formats a json object in the same way that python's json.dumps() formats.
pub struct PythonJsonFormatter;

impl Formatter for PythonJsonFormatter {
    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> IOResult<()>
    where
        W: ?Sized + Write,
    {
        CompactFormatter.begin_array_value(writer, first)?;
        if first { Ok(()) } else { writer.write_all(b" ") }
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> IOResult<()>
    where
        W: ?Sized + Write,
    {
        CompactFormatter.begin_object_key(writer, first)?;
        if first { Ok(()) } else { writer.write_all(b" ") }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> IOResult<()>
    where
        W: ?Sized + Write,
    {
        CompactFormatter.begin_object_value(writer)?;
        writer.write_all(b" ")
    }

    fn write_string_fragment<W>(&mut self, writer: &mut W, fragment: &str) -> IOResult<()>
    where
        W: ?Sized + Write,
    {
        let mut buf = [0u16; 2];
        for ch in fragment.chars() {
            if ch.is_ascii() {
                writer.write_all(&[ch as u8])?;
            } else {
                let slice = ch.encode_utf16(&mut buf);
                for num in slice {
                    write!(writer, r"\u{:4x}", num)?;
                }
            }
        }
        Ok(())
    }
}
