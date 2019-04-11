use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PubMessage {
    pub channel: String,
    pub payload: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AckMessage {
    ack: bool,
}

impl AckMessage {
    pub fn new() -> Self {
        Self { ack: true }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Subscribe {
    pub channel: String,
}
