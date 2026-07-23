use crate::network::{Network, CHANNEL_CAPACITY};
use crate::protocol::Message;

use anyhow::Context;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Duration;

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(handle) = &self._handle {
            handle.abort();
        }
    }
}

pub struct Client {
    server_addr: String,
    username: String,
    user_id: String,
    msg_rx: Option<mpsc::Receiver<Message>>,
    msg_tx: mpsc::Sender<Message>,
    broadcast_tx: mpsc::Sender<Message>,
    broadcast_rx: Option<mpsc::Receiver<Message>>,
    status: String,
    _handle: Option<JoinHandle<()>>,
}

impl Client {
    pub fn new(server_addr: String, username: String, user_id: String) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (broadcast_tx, broadcast_rx) = mpsc::channel(CHANNEL_CAPACITY);
        Self {
            server_addr,
            username,
            user_id,
            msg_rx: Some(msg_rx),
            msg_tx,
            broadcast_tx,
            broadcast_rx: Some(broadcast_rx),
            status: String::new(),
            _handle: None,
        }
    }
}

#[async_trait]
impl Network for Client {
    fn status_line(&self) -> String {
        if self.status.is_empty() {
            format!(" Connecting to {} ", self.server_addr)
        } else {
            format!(" {} ", self.status)
        }
    }

    fn take_receiver(&mut self) -> Option<mpsc::Receiver<Message>> {
        self.msg_rx.take()
    }

    fn broadcast_sender(&self) -> mpsc::Sender<Message> {
        self.broadcast_tx.clone()
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        self.status = format!("Connecting to {}...", self.server_addr);

        let stream = TcpStream::connect(&self.server_addr)
            .await
            .with_context(|| format!("failed to connect to {}", self.server_addr))?;
        self.status = format!("Connected to {}", self.server_addr);

        let addr = self.server_addr.clone();
        let msg_tx = self.msg_tx.clone();
        let broadcast_rx = self.broadcast_rx.take().unwrap();
        let join_msg = Message::Join {
            peer: crate::protocol::PeerInfo {
                id: self.user_id.clone(),
                name: self.username.clone(),
            },
        };

        self._handle = Some(tokio::spawn(client_loop(addr, join_msg, msg_tx, broadcast_rx, stream)));

        Ok(())
    }
}

async fn client_loop(
    server_addr: String,
    join_msg: Message,
    msg_tx: mpsc::Sender<Message>,
    mut broadcast_rx: mpsc::Receiver<Message>,
    mut stream: TcpStream,
) {
    let join_json = match prepare_json(&join_msg) {
        Some(j) => j,
        None => return,
    };

    loop {
        if stream.write_all(&join_json).await.is_err() {
            log::warn!("Failed to send Join, retrying...");
            match reconnect(&server_addr).await {
                Some(s) => stream = s,
                None => return,
            }
            continue;
        }

            if run_session(&mut stream, &mut broadcast_rx, &msg_tx).await {
            return;
        }

        match reconnect(&server_addr).await {
            Some(s) => {
                stream = s;
                log::info!("Reconnected to {server_addr}");
            }
            None => return,
        }
    }
}

async fn run_session(
    stream: &mut TcpStream,
    broadcast_rx: &mut mpsc::Receiver<Message>,
    msg_tx: &mpsc::Sender<Message>,
) -> bool {
    let mut buf_reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        tokio::select! {
            result = buf_reader.read_line(&mut line) => {
                line.clear();
                match result {
                    Ok(0) | Err(_) => return false,
                    Ok(_) => {
                        if line.ends_with('\n') {
                            line.pop();
                            if line.ends_with('\r') {
                                line.pop();
                            }
                        }
                        if let Ok(msg) = serde_json::from_str::<Message>(&line) {
                            let _ = msg_tx.try_send(msg);
                        }
                        line.clear();
                    }
                }
            }
            msg = broadcast_rx.recv() => {
                match msg {
                    Some(msg) => {
                        let json = match prepare_json(&msg) {
                            Some(j) => j,
                            None => continue,
                        };
                        if buf_reader.get_mut().write_all(&json).await.is_err() {
                            return false;
                        }
                        let _ = msg_tx.try_send(msg);
                    }
                    None => return true,
                }
            }
        }
    }
}

fn prepare_json(msg: &Message) -> Option<Vec<u8>> {
    let mut json = serde_json::to_string(msg).unwrap_or_else(|e| {
        log::error!("failed to serialize msg: {e}");
        String::new()
    });
    if json.is_empty() {
        return None;
    }
    json.push('\n');
    Some(json.into_bytes())
}

async fn reconnect(server_addr: &str) -> Option<TcpStream> {
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        match TcpStream::connect(server_addr).await {
            Ok(s) => return Some(s),
            Err(e) => log::warn!("Reconnect failed: {e}"),
        }
    }
}
