use crate::network::{Client, Network, Server};
use crate::protocol::Message;
use tokio::sync::mpsc;

const MAX_MESSAGES: usize = 1000;

pub struct ChatMessage {
    pub time: String,
    pub sender: String,
    pub content: String,
}

pub struct MenuState {
    pub server_addr: String,
    pub show_input: bool,
    pub show_help: bool,
    pub connecting: bool,
    pub error: Option<String>,
}

pub enum AppMode {
    Menu(MenuState),
    Chat,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub username: String,
    pub quit: bool,
    pub mode: AppMode,
    network: Option<Box<dyn Network>>,
    msg_rx: Option<mpsc::UnboundedReceiver<Message>>,
    broadcast_tx: Option<mpsc::UnboundedSender<Message>>,
}

impl App {
    pub fn new(username: String) -> Self {
        Self {
            messages: Vec::with_capacity(MAX_MESSAGES),
            input: String::new(),
            username,
            quit: false,
            mode: AppMode::Menu(MenuState {
                server_addr: String::new(),
                show_input: false,
                show_help: false,
                connecting: false,
                error: None,
            }),
            network: None,
            msg_rx: None,
            broadcast_tx: None,
        }
    }

    pub fn status_line(&self) -> String {
        self.network
            .as_ref()
            .map(|n| n.status_line())
            .unwrap_or_default()
    }

    pub async fn start_server(&mut self) -> anyhow::Result<()> {
        let mut network = Box::new(Server::new(self.username.clone()));
        let msg_rx = network.take_receiver();
        let broadcast_tx = network.broadcast_sender();
        network.start().await?;
        self.network = Some(network);
        self.msg_rx = msg_rx;
        self.broadcast_tx = Some(broadcast_tx);
        self.mode = AppMode::Chat;
        Ok(())
    }

    pub async fn connect_to(&mut self, addr: String) -> anyhow::Result<()> {
        let mut network = Box::new(Client::new(addr, self.username.clone()));
        let msg_rx = network.take_receiver();
        let broadcast_tx = network.broadcast_sender();
        network.start().await?;
        self.network = Some(network);
        self.msg_rx = msg_rx;
        self.broadcast_tx = Some(broadcast_tx);
        self.mode = AppMode::Chat;
        Ok(())
    }

    pub fn send_message(&mut self) {
        let content = self.input.trim().to_string();
        if content.is_empty() {
            return;
        }
        push_with_cap(
            &mut self.messages,
            ChatMessage {
                time: Self::current_time(),
                sender: self.username.clone(),
                content: content.clone(),
            },
        );
        self.input.clear();
        if let Some(tx) = &self.broadcast_tx {
            let msg = Message::Text {
                sender: self.username.clone(),
                content,
            };
            let _ = tx.send(msg);
        }
    }

    pub fn poll_messages(&mut self) {
        if let Some(rx) = &mut self.msg_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::Text { sender, content } => {
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender,
                                content,
                            },
                        );
                    }
                    Message::Join { peer } => {
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender: "SYSTEM".into(),
                                content: format!("{} joined", peer.name),
                            },
                        );
                    }
                    Message::Leave { peer } => {
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender: "SYSTEM".into(),
                                content: format!("{} left", peer.name),
                            },
                        );
                    }
                    Message::UserList { peers } => {
                        let names: Vec<&str> = peers.iter().map(|p| p.name.as_str()).collect();
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender: "SYSTEM".into(),
                                content: format!("Online: {}", names.join(", ")),
                            },
                        );
                    }
                }
            }
        }
    }

    pub fn current_time() -> String {
        if let Ok(output) = std::process::Command::new("date").arg("+%H:%M:%S").output() {
            if let Ok(s) = String::from_utf8(output.stdout) {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    return s;
                }
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}

fn push_with_cap(vec: &mut Vec<ChatMessage>, msg: ChatMessage) {
    if vec.len() >= MAX_MESSAGES {
        vec.remove(0);
    }
    vec.push(msg);
}
