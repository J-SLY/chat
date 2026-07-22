use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Text { sender: String, content: String },
    Join { peer: PeerInfo },
    Leave { peer: PeerInfo },
    UserList { peers: Vec<PeerInfo> },
}
