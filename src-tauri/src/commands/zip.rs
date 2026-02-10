use crate::error::{DromeError, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

pub fn zip_compress(input: String) -> Result<Vec<u8>> {
  let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
  encoder.write_all(input.as_bytes())?;
  let out = encoder.finish()?;
  Ok(out)
}

pub fn zip_decompress(bytes: Vec<u8>) -> Result<String> {
  let mut decoder = GzDecoder::new(bytes.as_slice());
  let mut out = String::new();
  decoder.read_to_string(&mut out).map_err(|e| DromeError::Message(e.to_string()))?;
  Ok(out)
}

