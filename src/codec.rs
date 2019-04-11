use bytes::{Bytes, BytesMut};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::error::Result;

use crate::BUFFER_SIZE;

pub struct LineCodec;

impl LineCodec {
    pub fn decode<T: DeserializeOwned>(buf: &mut BytesMut) -> Option<T> {
        let p = buf.iter().position(|b| b == &b'\n');
        match p {
            None => None,
            Some(n) => {
                let res = serde_json::from_slice(&buf.split_to(n).freeze()).ok();
                buf.advance(1); // Skip the newline char
                buf.reserve(BUFFER_SIZE); // Make sure the buffer can hold more data
                res
            }
        }
    }

    pub fn encode<T: Serialize>(t: T) -> Result<Bytes> {
        let mut payload = serde_json::to_vec(&t)?;
        payload.push(b'\n');
        Ok(Bytes::from(payload))
    }
}
