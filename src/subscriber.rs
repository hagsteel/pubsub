use std::collections::HashMap;

use sonr::Token;
use sonr::reactor::{Reactor, Reaction};
use sonr::errors::Result;
use sonr::net::tcp::{TcpStream, ReactiveTcpStream};
use sonr::sync::signal::{SignalReceiver, ReactiveSignalReceiver};
use bytes::{Bytes, BytesMut, BufMut};

use crate::connection::Connection;
use crate::codec::LineCodec;
use crate::messages::{PubMessage, Subscribe};
use crate::BUFFER_SIZE;

pub struct Subscriber {
    connections: HashMap<Token, Connection>,
    messages: ReactiveSignalReceiver<Bytes>,
    channels: HashMap<String, Vec<Token>>,
    message_buffer: BytesMut,
}

impl Subscriber {
    pub fn new(messages: SignalReceiver<Bytes>) -> Result<Self> {
        Ok(Self {
            connections: HashMap::new(),
            messages: ReactiveSignalReceiver::new(messages)?,
            channels: HashMap::new(),
            message_buffer: BytesMut::with_capacity(BUFFER_SIZE),
        })
    }

    fn publish(&mut self) {
        loop {
            let message = LineCodec::decode::<PubMessage>(&mut self.message_buffer);
            if message.is_none() { return }

            let message = message.unwrap();
            if let Some(connection_ids) = self.channels.get(&message.channel) {
                let connection_ids = connection_ids.clone();

                if let Ok(encoded_message) = LineCodec::encode(message) {
                    for cid in connection_ids {
                        if let Some(con) = self.connections.get_mut(&cid) {
                            con.add_payload(encoded_message.clone());

                            while let Some(wrt_res) = con.write() {
                                if wrt_res.is_err() {
                                    self.connections.remove(&cid);
                                    self.unsubscribe(cid);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn unsubscribe(&mut self, connection_id: Token) {
        for connection_ids in self.channels.values_mut() {
            while let Some(pos) = connection_ids.iter().position(|id| id == &connection_id) {
                connection_ids.remove(pos);
            }
        }
    }

    fn add_payload(&mut self, payload: Bytes) {
        if payload.len() > self.message_buffer.remaining_mut() {
            self.message_buffer.reserve(payload.len());
        }
        self.message_buffer.put_slice(&payload);
    }
}

impl Reactor for Subscriber {
    type Input = TcpStream;
    type Output = ();

    fn react(&mut self, reaction: Reaction<Self::Input>) -> Reaction<Self::Output> {
        use Reaction::*;
        match reaction {
            Value(stream) => {
                if let Ok(stream) = ReactiveTcpStream::new(stream) {
                    self.connections.insert(stream.token(), Connection::new(stream));
                }
                Continue
            }
            Event(event) => {
                // Incoming messages:
                if self.messages.token() == event.token() {
                    if let Value(messages) = self.messages.react(event.into()) {
                        self.add_payload(messages);
                        
                        // Keep "reacting" until we no longer receive a message
                        while let Value(messages) = self.messages.react(Continue) {
                            self.add_payload(messages);
                        }

                        self.publish();
                    }
                    return Continue
                }

                // Connection event:
                if let Some(con) = self.connections.get_mut(&event.token()) {
                    con.react(event.into());

                    // Read all "subscribe" messages
                    while let Some(messages) = con.recv::<Subscribe>() {
                        match messages {
                            Ok(messages) => {
                                for message in messages {
                                    match self.channels.get_mut(&message.channel) {
                                        Some(_) => {
                                            if let Some(conneciton_ids) = self.channels.get_mut(&message.channel) {
                                                conneciton_ids.push(event.token());
                                            }
                                        }
                                        None => { self.channels.insert(message.channel, vec![event.token()]); }
                                    }
                                }
                            }
                            Err(_) => {
                                self.connections.remove(&event.token());
                                self.unsubscribe(event.token());
                                return Continue
                            }
                        }
                    }
                    Continue
                } else {
                    event.into()
                }
            }
            Continue => Continue,
        }
    }
}
