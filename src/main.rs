use std::thread;
use std::time::Duration;
use sonr::prelude::*;
use sonr::errors::Result;
use sonr::net::tcp::{TcpStream, ReactiveTcpListener};
use sonr::sync::broadcast::Broadcast;
use sonr::sync::queue::{ReactiveQueue, ReactiveDeque};

use pubsub::publisher::Publisher;
use pubsub::subscriber::Subscriber;
use pubsub::timer::Timer;

fn listener(addr: &str) -> Result<impl Reactor<Output=TcpStream>> {
    let l = ReactiveTcpListener::bind(addr)?
        .map(|(s, _)| s);
    Ok(l)
}


fn main() -> Result<()> {
    System::init()?;
    let thread_count = 8;
    let buffer_threshold = 256;
    let publish_timeout = Duration::from_millis(20);

    // Publisher
    let pub_listener = listener("127.0.0.1:8000")?;
    let broadcast = Broadcast::unbounded();
    let mut timer = Timer::new(publish_timeout);
    let mut pub_connection_queue = ReactiveQueue::unbounded(); 

    // Subscriber
    let sub_listener = listener("127.0.0.1:9000")?;
    let mut sub_connection_queue = ReactiveQueue::unbounded(); 

    for _ in 0..thread_count {
        let broadcast = broadcast.clone();
        let timer_notifier = timer.receiver();
        let pub_deque = pub_connection_queue.deque();
        let sub_deque = sub_connection_queue.deque();

        thread::spawn(move || -> Result<()> {
            System::init()?;

            let sub_connection_deque = ReactiveDeque::new(sub_deque)?;
            let subscriber = Subscriber::new(broadcast.subscriber())?;
            let sub_run = sub_connection_deque.chain(subscriber);

            let pub_connection_deque = ReactiveDeque::new(pub_deque)?;
            let publisher = Publisher::new(broadcast, buffer_threshold, timer_notifier)?;
            let pub_run = pub_connection_deque.chain(publisher);

            let run = pub_run.and(sub_run);

            System::start(run)?;

            Ok(())
        });
    }

    timer.start();

    let pub_run = pub_listener.chain(pub_connection_queue);
    let sub_run = sub_listener.chain(sub_connection_queue);

    System::start(pub_run.and(sub_run))?;

    Ok(())
}
