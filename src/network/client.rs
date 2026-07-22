use crate::network::Network;
use crate::protocol::Message;

use anyhow::Context;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

pub struct Client {
    server_addr: String,
    username: String,
    msg_rx: Option<mpsc::UnboundedReceiver<Message>>,
    msg_tx: mpsc::UnboundedSender<Message>,
    broadcast_tx: mpsc::UnboundedSender<Message>,
    broadcast_rx: Option<mpsc::UnboundedReceiver<Message>>,
    status: String,
    _handle: Option<JoinHandle<()>>,
}

impl Client {
    pub fn new(server_addr: String, username: String) -> Self {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, broadcast_rx) = mpsc::unbounded_channel();
        Self {
            server_addr,
            username,
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

    fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<Message>> {
        self.msg_rx.take()
    }

    fn broadcast_sender(&self) -> mpsc::UnboundedSender<Message> {
        self.broadcast_tx.clone()
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        self.status = format!("Connecting to {}...", self.server_addr);
        let stream = TcpStream::connect(&self.server_addr)
            .await
            .with_context(|| format!("failed to connect to {}", self.server_addr))?;
        self.status = format!("Connected to {}", self.server_addr);

        let (reader, writer) = stream.into_split();
        let msg_tx = self.msg_tx.clone();
        let msg_tx2 = msg_tx.clone();
        let broadcast_rx = self.broadcast_rx.take().unwrap();

        // send Join
        let join_msg = Message::Join {
            peer: crate::protocol::PeerInfo {
                name: self.username.clone(),
            },
        };
        let mut writer_clone = writer;
        let mut json = serde_json::to_string(&join_msg).unwrap();
        json.push('\n');
        writer_clone
            .write_all(json.as_bytes())
            .await
            .context("failed to send Join")?;

        // writer task: broadcast_tx → server, echo back to self
        let writer_handle = tokio::spawn(async move {
            let mut rx = broadcast_rx;
            let mut writer = writer_clone;
            while let Some(msg) = rx.recv().await {
                let mut json = serde_json::to_string(&msg).unwrap();
                json.push('\n');
                if writer.write_all(json.as_bytes()).await.is_err() {
                    break;
                }
                let _ = msg_tx2.send(msg);
            }
        });

        // reader task: server → msg_tx
        let reader_handle = tokio::spawn(async move {
            let mut buf_reader = BufReader::new(reader);
            let mut line_buf = String::new();
            loop {
                line_buf.clear();
                match buf_reader.read_line(&mut line_buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line_buf.ends_with('\n') {
                            line_buf.pop();
                            if line_buf.ends_with('\r') {
                                line_buf.pop();
                            }
                        }
                        if let Ok(msg) = serde_json::from_str::<Message>(&line_buf) {
                            let _ = msg_tx.send(msg);
                        }
                    }
                }
            }
        });

        self._handle = Some(tokio::spawn(async move {
            let _ = writer_handle.await;
            let _ = reader_handle.await;
        }));

        Ok(())
    }
}
