use std::collections::HashMap;

use sonr::reactor::{Reactor, Reaction};
use sonr::sync::broadcast::Broadcast;
use sonr::net::tcp::{ReactiveTcpStream, TcpStream};
use sonr::Token;
use sonr::errors::Result;
use bytes::{Bytes, BytesMut, BufMut};

use crate::connection::Connection;
use crate::codec::LineCodec;
use crate::messages::{PubMessage, AckMessage};
use crate::timer::{TimerNotifier, ReactiveTimerNotifier};


pub struct Publisher {
    connections: HashMap<Token, Connection>,
    broadcast: Broadcast<Bytes>,
    buffer_threshold: usize, // buffer messages
    publish_payload: BytesMut,
    timer: ReactiveTimerNotifier,
}

impl Publisher {
    pub fn new(broadcast: Broadcast<Bytes>, buffer_threshold: usize, timer: TimerNotifier) -> Result<Self> {
        let timer = ReactiveTimerNotifier::new(timer)?;

        Ok(Self {  
            connections: HashMap::new(),
            broadcast,
            buffer_threshold,
            publish_payload: BytesMut::with_capacity(buffer_threshold * 2),
            timer,
        })
    }
}

impl Reactor for Publisher {
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
                // Timer tick event:
                if event.token() == self.timer.token() {
                    // We can ignore the result as it's simply a unit,
                    // however we should get the result out to make room 
                    // for the next one.
                    let _ = self.timer.try_recv();

                    // Only publish if we have actual data
                    if !self.publish_payload.is_empty() {
                        self.broadcast.publish(self.publish_payload.take().freeze());
                    }
                }

                // Connection event:
                if let Some(con) = self.connections.get_mut(&event.token()) {
                    con.react(event.into()); // Mark the underlying stream as readable / writable

                    // Read messages to publish.
                    // Note: could simply send an "ack" message for every "\n"
                    // char, however this ensures that the message is an actual `PubMessage`
                    while let Some(msg_result) = con.recv::<PubMessage>() {
                        match msg_result {
                            Ok(messages) => {
                                for message in messages {
                                    if let Ok(bytes) = LineCodec::encode(&message) {
                                        if bytes.len() > self.publish_payload.remaining_mut() {
                                            self.publish_payload.reserve(self.buffer_threshold);
                                        }
                                        self.publish_payload.put_slice(&bytes);
                                    }

                                    // ack message
                                    let _ = LineCodec::encode(AckMessage::new()).map(|payload| con.add_payload(payload));
                                }
                            }
                            Err(_) => {
                                self.connections.remove(&event.token());
                                
                                // Publish the payload
                                self.broadcast.publish(self.publish_payload.take().freeze());
                                return Continue
                            }
                        }
                    }

                    // If enough data is buffered then publish the messages.
                    if self.publish_payload.len() >= self.buffer_threshold {
                        self.broadcast.publish(self.publish_payload.take().freeze());
                    }

                    // Write all ack messages
                    while let Some(wrt_res) = con.write() {
                        if wrt_res.is_err() {
                            self.connections.remove(&event.token());
                            return Continue
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
