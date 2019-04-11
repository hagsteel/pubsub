use sonr::sync::signal::{SignalSender, SignalReceiver, ReactiveSignalReceiver};
use std::thread;
use std::time::Duration;

pub type ReactiveTimerNotifier = ReactiveSignalReceiver<()>;
pub type TimerNotifier = SignalReceiver<()>;

pub struct Timer {
    senders: Vec<SignalSender<()>>,
    interval: Duration,
}

impl Timer {
    pub fn new(interval: Duration) -> Self {
        Self { 
            senders: Vec::new(),
            interval,
        }
    }

    pub fn receiver(&mut self) -> SignalReceiver<()> {
        let receiver = SignalReceiver::bounded(1);
        self.senders.push(receiver.sender());
        receiver
    }

    pub fn start(self) {
        thread::spawn(move || {
            loop {
                thread::sleep(self.interval);
                self.senders.iter().for_each(|n| { let _ = n.send(()); } );
            }
        });
    }
}
