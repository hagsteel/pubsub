use std::time::Duration;
use std::collections::HashMap;
use std::thread;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::net::SocketAddr;

use sonr::errors::Result;
use sonr::net::tcp::{ReactiveTcpStream, TcpStream};
use sonr::prelude::*;

use pubsub::connection::Connection;
use pubsub::messages::{Subscribe, PubMessage};
use pubsub::codec::LineCodec;

static COUNTER: AtomicUsize = AtomicUsize::new(0);
 
struct Connections {
    connections: HashMap<Token, Connection>,
}

impl Connections {
    pub fn new(con_count: usize) -> Self {
        let addr: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        let connections = (0..con_count).map(|_| {
            let stream = TcpStream::connect(&addr).unwrap();
            let stream = ReactiveTcpStream::new(stream).unwrap();
            let token = stream.token();
            let mut con = Connection::new(stream);
            let payload = LineCodec::encode(Subscribe { channel: "abc".to_owned() }).unwrap();
            con.add_payload(payload);
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

                    while let Some(messages) = connection.recv::<PubMessage>() {
                        match messages {
                            Ok(msg) => {
                                COUNTER.fetch_add(msg.len(), Ordering::SeqCst);
                            }
                            Err(_) => {
                                self.connections.remove(&connection_id);
                                return Continue
                            }
                        }
                    }

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
    eprintln!("{} msg/s", msg_per_sec);
    eprintln!("{} messages", count);
} 
