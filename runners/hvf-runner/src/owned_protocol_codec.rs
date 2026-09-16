//! Canonical bytes reject duplicates, missing nulls, unknown fields and loose spellings.
use super::dto::{Hello, Stop};
use serde::{de::DeserializeOwned, Serialize};
use std::io;
pub(super) const MAX_FRAME: usize = 8192;
pub(super) fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid owned protocol frame")
}
pub(super) fn encode<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let value = serde_json::to_value(value).map_err(|_| invalid())?;
    let bytes = serde_json::to_vec(&value).map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME || !bytes.is_ascii() {
        return Err(invalid());
    }
    Ok(bytes)
}
pub(super) fn decode<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> io::Result<T> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME || !bytes.is_ascii() {
        return Err(invalid());
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if encode(&value)? != bytes {
        return Err(invalid());
    }
    Ok(value)
}
pub(super) fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
pub(super) fn hello(bytes: &[u8], digest: &str, needs_key: bool) -> io::Result<Hello> {
    let value: Hello = decode(bytes)?;
    if value.schema_version != 1
        || value.kind != "hello"
        || value.sequence != 0
        || !uuid(&value.run_token)
        || !hex(&value.manifest_sha256, 64)
        || value.manifest_sha256 != digest
        || value.key_hex.is_some() != needs_key
        || value.key_hex.as_ref().is_some_and(|key| !hex(key, 64))
    {
        return Err(invalid());
    }
    Ok(value)
}
pub(super) fn stop(bytes: &[u8], token: &str) -> io::Result<Stop> {
    let value: Stop = decode(bytes)?;
    if value.schema_version != 1
        || value.kind != "stop"
        || value.sequence != 1
        || value.run_token != token
        || !uuid(&value.operation_id)
    {
        return Err(invalid());
    }
    Ok(value)
}
pub(super) fn take_key(hello: &mut Hello) -> Option<Vec<u8>> {
    hello.key_hex.take().map(|text| {
        let mut text = text.into_bytes();
        let result = text
            .chunks_exact(2)
            .map(|pair| {
                fn digit(b: u8) -> u8 {
                    if b.is_ascii_digit() {
                        b - b'0'
                    } else {
                        b - b'a' + 10
                    }
                }
                digit(pair[0]) * 16 + digit(pair[1])
            })
            .collect();
        text.fill(0);
        result
    })
}
#[cfg(test)]
#[path = "owned_protocol_codec_tests.rs"]
mod tests;
