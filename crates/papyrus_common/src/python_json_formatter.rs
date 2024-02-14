use std::io::{Result as IOResult, Write};

use serde_json::ser::{CompactFormatter, Formatter};

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
