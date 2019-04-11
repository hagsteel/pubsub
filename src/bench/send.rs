use std::time::Duration;
use std::collections::HashMap;
use std::thread;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::net::SocketAddr;
use bytes::{Bytes, BytesMut, BufMut};

use sonr::errors::Result;
use sonr::net::tcp::{ReactiveTcpStream, TcpStream};
use sonr::prelude::*;

use pubsub::connection::Connection;
use pubsub::messages::AckMessage;

static COUNTER: AtomicUsize = AtomicUsize::new(0);
static FAIL_COUNTER: AtomicUsize = AtomicUsize::new(0);
const SEND_TRIGGER: usize = 256;
static PAYLOAD: &'static [u8] = b"{\"channel\":\"abc\",\"payload\":\"hello\"}\n";

fn payload(count: usize) -> Bytes {
    let mut b = BytesMut::with_capacity(count * PAYLOAD.len());
    (0..count).for_each(|_| {
        b.put_slice(PAYLOAD)
    });

    b.freeze()
}
 
struct Connections {
    connections: HashMap<Token, Connection>,
}

impl Connections {
    pub fn new(con_count: usize) -> Self {
        let addr: SocketAddr = "127.0.0.1:8000".parse().unwrap();
        let connections = (0..con_count).map(|_| {
            let stream = TcpStream::connect(&addr).unwrap();
            let stream = ReactiveTcpStream::new(stream).unwrap();
            let token = stream.token();
            let mut con = Connection::new(stream);
            con.add_payload(payload(SEND_TRIGGER));
            (token, con)
        }).collect::<HashMap<Token, Connection>>();

        Self {
            connections,
        }
    }
}

impl Reactor for Connections {
    type Input = ();
    type Output = ();

    fn react(&mut self, reaction: Reaction<Self::Input>) -> Reaction<Self::Output> {
        use Reaction::*;
        match reaction {
            Event(event) => {
                let connection_id = event.token();
                if let Some(connection) = self.connections.get_mut(&connection_id) {
                    connection.react(event.into());

                    let mut ok_msg_count = 0usize;
                    while let Some(messages) = connection.recv::<AckMessage>() {
                        match messages {
                            Ok(msg) => {
                                COUNTER.fetch_add(msg.len(), Ordering::SeqCst);
                                ok_msg_count += msg.len();
                            }
                            Err(_) => {
                                self.connections.remove(&connection_id);
                                return Continue
                            }
                        }
                    }

                    connection.add_payload(payload(ok_msg_count));

                    while let Some(res) = connection.write() {
                        if res.is_err() {
                            self.connections.remove(&connection_id);
                            return Continue;
                        }
                    }

                    Continue
                } else {
                    event.into()
                }
            }
            Value(()) => Continue,
            Continue => Continue
        }
    }
}

fn main() {
    let thread_count = 8;
    let con_per_thread = 1;
    let mut handles = Vec::new();

    for _ in 0..thread_count {
        let handle = thread::spawn(move || -> Result<()> {
            System::init()?;
            let subscribing = Connections::new(con_per_thread);
            let run = subscribing;
            System::start(run)?;
            Ok(())
        });

        handles.push(handle);
    }

    let mul = 8usize;
    thread::sleep(Duration::from_secs(mul as u64));

    let count = COUNTER.load(Ordering::SeqCst);
    let msg_per_sec = count / mul;
    let fail_count = FAIL_COUNTER.load(Ordering::SeqCst);
    let payload = payload(1);
    let mb = (count * payload.len()) / 1024 / 1024;
    eprintln!("{} msg/s", msg_per_sec);
    eprintln!("{} messages", count);
    eprintln!("{} messages failed", fail_count);
    eprintln!("{} MB/s", mb / mul);
} 
