use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Text { sender_id: String, sender_name: String, content: String },
    Join { peer: PeerInfo },
    Leave { peer: PeerInfo },
    UserList { peers: Vec<PeerInfo> },
}
