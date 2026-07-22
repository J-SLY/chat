use crate::network::Network;
use crate::protocol::{Message, PeerInfo};

use anyhow::Context;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

struct ClientConn {
    info: PeerInfo,
    tx: mpsc::UnboundedSender<Message>,
}

fn make_id(name: &str, addr: &str) -> String {
    format!("{}@{}", name, addr)
}

pub struct Server {
    bind_addr: String,
    clients: Arc<RwLock<HashMap<String, ClientConn>>>,
    msg_rx: Option<mpsc::UnboundedReceiver<Message>>,
    msg_tx: mpsc::UnboundedSender<Message>,
    broadcast_tx: mpsc::UnboundedSender<Message>,
    broadcast_rx: Option<mpsc::UnboundedReceiver<Message>>,
    client_count: Arc<std::sync::atomic::AtomicUsize>,
    _handles: Vec<JoinHandle<()>>,
}

impl Server {
    pub fn new(_username: String) -> Self {
        let port = crate::config::port();
        let bind_addr = format!("0.0.0.0:{}", port);
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, broadcast_rx) = mpsc::unbounded_channel();

        Self {
            bind_addr,
            clients: Arc::new(RwLock::new(HashMap::new())),
            msg_rx: Some(msg_rx),
            msg_tx,
            broadcast_tx,
            broadcast_rx: Some(broadcast_rx),
            client_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            _handles: Vec::new(),
        }
    }
}

#[async_trait]
impl Network for Server {
    fn status_line(&self) -> String {
        let count = self.client_count.load(std::sync::atomic::Ordering::Relaxed);
        format!(" Server :{} | {} connected ", self.bind_addr, count)
    }

    fn take_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<Message>> {
        self.msg_rx.take()
    }

    fn broadcast_sender(&self) -> mpsc::UnboundedSender<Message> {
        self.broadcast_tx.clone()
    }

    async fn start(&mut self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(&self.bind_addr)
            .await
            .context("failed to bind server port")?;
        let local_addr = listener.local_addr().context("no local addr")?;
        log::info!("Server listening on {}", local_addr);

        let clients = self.clients.clone();
        let msg_tx = self.msg_tx.clone();
        let msg_tx_for_broadcast = self.msg_tx.clone();
        let client_count = self.client_count.clone();

        let accept_handle = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        log::info!("Accepted connection from {}", addr);
                        tokio::spawn(handle_connection(
                            stream,
                            addr.to_string(),
                            clients.clone(),
                            msg_tx.clone(),
                            client_count.clone(),
                        ));
                    }
                    Err(e) => log::error!("Accept error: {}", e),
                }
            }
        });

        // broadcast task: server user's own messages + web messages → all clients + app
        let broadcast_rx = self.broadcast_rx.take().unwrap();
        let clients_b = self.clients.clone();
        let bf_handle = tokio::spawn(async move {
            let mut rx = broadcast_rx;
            while let Some(msg) = rx.recv().await {
                let clients = clients_b.read().await;
                for (_id, conn) in clients.iter() {
                    let _ = conn.tx.send(msg.clone());
                }
                let _ = msg_tx_for_broadcast.send(msg);
            }
        });

        self._handles.push(accept_handle);
        self._handles.push(bf_handle);

        // LAN discovery: broadcast presence via UDP multicast
        let port = crate::config::port();
        crate::network::discovery::spawn_announcer(port);

        Ok(())
    }
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    remote_addr: String,
    clients: Arc<RwLock<HashMap<String, ClientConn>>>,
    msg_tx: mpsc::UnboundedSender<Message>,
    client_count: Arc<std::sync::atomic::AtomicUsize>,
) {
    let (reader, writer) = stream.into_split();
    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    let mut buf_reader = BufReader::new(reader);
    let mut first_line = String::new();

    // first message must be Join
    if buf_reader.read_line(&mut first_line).await.ok().filter(|&n| n > 0).is_none() {
        return;
    }
    trim_newline(&mut first_line);

    let join_msg: Message = match serde_json::from_str(&first_line) {
        Ok(m) => m,
        Err(_) => return,
    };
    let peer = match &join_msg {
        Message::Join { peer } => peer.clone(),
        _ => return,
    };

    let id = make_id(&peer.name, &remote_addr);
    {
        let mut map = clients.write().await;
        map.insert(
            id.clone(),
            ClientConn {
                info: peer.clone(),
                tx: conn_tx,
            },
        );
    }
    client_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    // notify server app
    let _ = msg_tx.send(Message::Join {
        peer: peer.clone(),
    });

    // notify other clients
    {
        let map = clients.read().await;
        let others: Vec<&ClientConn> = map.iter().filter(|(k, _)| *k != &id).map(|(_, v)| v).collect();
        for conn in &others {
            let _ = conn.tx.send(Message::Join {
                peer: peer.clone(),
            });
        }
        // send user list to the new client
        let users: Vec<PeerInfo> = map.values().map(|c| c.info.clone()).collect();
        if let Some(me) = map.get(&id) {
            let _ = me.tx.send(Message::UserList { peers: users });
        }
    }

    // writer task: forward messages from broadcast_rx to this client
    let _writer_handle: JoinHandle<()> = tokio::spawn(async move {
        let mut writer = writer;
        while let Some(msg) = conn_rx.recv().await {
            let mut json = serde_json::to_string(&msg).unwrap();
            json.push('\n');
            if writer.write_all(json.as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // read loop
    let mut line_buf = String::new();
    loop {
        line_buf.clear();
        match buf_reader.read_line(&mut line_buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                trim_newline(&mut line_buf);
                if let Ok(msg) = serde_json::from_str::<Message>(&line_buf) {
                    match &msg {
                        Message::Text { .. } => {
                            let _ = msg_tx.send(msg.clone());
                            let map = clients.read().await;
                            for (other_id, conn) in map.iter() {
                                if *other_id != id {
                                    let _ = conn.tx.send(msg.clone());
                                }
                            }
                        }
                        Message::Leave { .. } => break,
                        _ => {}
                    }
                }
            }
        }
    }

    // cleanup
    clients.write().await.remove(&id);
    client_count.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    let _ = msg_tx.send(Message::Leave {
        peer: peer.clone(),
    });
    let map = clients.read().await;
    for (_other_id, conn) in map.iter() {
        let _ = conn.tx.send(Message::Leave {
            peer: peer.clone(),
        });
    }
}

fn trim_newline(s: &mut String) {
    if s.ends_with('\n') {
        s.pop();
        if s.ends_with('\r') {
            s.pop();
        }
    }
}
