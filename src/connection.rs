use bytes::{Bytes, BufMut, BytesMut};
use serde::de::DeserializeOwned;
use sonr::net::tcp::ReactiveTcpStream;
use sonr::reactor::{Reaction, Reactor};
use std::io::{ErrorKind::WouldBlock, Read, Write};

use crate::codec::LineCodec;
use crate::BUFFER_SIZE;

pub struct Connection {
    stream: ReactiveTcpStream,
    read_buffer: BytesMut,
    write_buffer: BytesMut,
}

impl Connection {
    pub fn new(stream: ReactiveTcpStream) -> Self {
        Self {
            stream,
            read_buffer: BytesMut::with_capacity(BUFFER_SIZE),
            write_buffer: BytesMut::with_capacity(BUFFER_SIZE),
        }
    }

    pub fn recv<T: DeserializeOwned>(&mut self) -> Option<Result<Vec<T>, ()>> {
        if !self.stream.readable() {
            return None;
        }

        let res = {
            let mut b = unsafe { self.read_buffer.bytes_mut() };
            self.stream.read(&mut b)
        };

        match res {
            // The connection was closed by the peer.
            Ok(0) => Some(Err(())),

            // Try to decode messages from the read data
            Ok(n) => {
                let buf_len = self.read_buffer.len() + n;
                unsafe { self.read_buffer.set_len(buf_len); }

                let mut v = Vec::new();
                while let Some(val) = LineCodec::decode(&mut self.read_buffer) {
                    v.push(val);
                }

                Some(Ok(v))
            }
            
            // Not an actual error
            Err(ref e) if e.kind() == WouldBlock => None,

            // Connection closed. Ignoring the reason
            // for simplicity
            Err(_) => Some(Err(())),
        }
    }

    pub fn write(&mut self) -> Option<Result<usize, ()>> {
        if !self.stream.writable() {
            return None
        } 

        if self.write_buffer.is_empty() {
            return None
        }

        match self.stream.write(&self.write_buffer) {
            Ok(n) => {
                self.write_buffer.split_to(n); // Remove sent data
                Some(Ok(n))
            }
            Err(ref e) if e.kind() == WouldBlock => None,
            Err(_) => Some(Err(())),
        }
    }

    pub fn add_payload(&mut self, payload: Bytes) {
        if payload.len() > self.write_buffer.remaining_mut() {
            self.write_buffer.reserve(payload.len());
        }
        self.write_buffer.put_slice(&payload);
    }

    // Convenience, saving us from having to make the stream public
    pub fn react(&mut self, reaction: Reaction<()>) -> Reaction<()> {
        self.stream.react(reaction)
    }
}
