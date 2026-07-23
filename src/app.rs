use crate::config;
use crate::network::{Client, Network, Server, CHANNEL_CAPACITY};
use crate::protocol::{Message, PeerInfo};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const MAX_MESSAGES: usize = 1000;

pub struct ChatMessage {
    pub time: String,
    pub sender: String,
    pub sender_id: String,
    pub content: String,
}

pub struct MenuState {
    pub server_addr: String,
    pub server_cursor: usize,
    pub show_input: bool,
    pub show_help: bool,
    pub connecting: bool,
    pub error: Option<String>,
    pub discovered_servers: Vec<(String, std::time::Instant)>,
    pub show_settings: bool,
    pub edit_username: bool,
    pub username_input: String,
    pub username_cursor: usize,
}

pub enum AppMode {
    Setup,
    Menu(MenuState),
    Chat,
}

pub struct App {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub cursor: usize,
    pub username: String,
    pub user_id: String,
    pub quit: bool,
    pub mode: AppMode,
    pub online_users: Vec<String>,
    network: Option<Box<dyn Network>>,
    msg_rx: Option<mpsc::Receiver<Message>>,
    broadcast_tx: Option<mpsc::Sender<Message>>,
    discovery_rx: Option<mpsc::Receiver<String>>,
    _discovery_handles: Vec<JoinHandle<()>>,
}

impl App {
    pub fn new(username: String, user_id: String) -> Self {
        let (discovery_tx, discovery_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let listener_handle = crate::network::discovery::spawn_listener(discovery_tx);
        Self {
            messages: Vec::with_capacity(MAX_MESSAGES),
            input: String::new(),
            cursor: 0,
            username,
            user_id,
            quit: false,
            online_users: Vec::new(),
            mode: AppMode::Menu(MenuState {
                server_addr: String::new(),
                server_cursor: 0,
                show_input: false,
                show_help: false,
                connecting: false,
                error: None,
                discovered_servers: Vec::new(),
                show_settings: false,
                edit_username: false,
                username_input: String::new(),
                username_cursor: 0,
            }),
            network: None,
            msg_rx: None,
            broadcast_tx: None,
            discovery_rx: Some(discovery_rx),
            _discovery_handles: vec![listener_handle],
        }
    }

    pub fn status_line(&self) -> String {
        self.network
            .as_ref()
            .map(|n| n.status_line())
            .unwrap_or_default()
    }

    pub async fn start_server(&mut self) -> anyhow::Result<()> {
        let mut network = Box::new(Server::new(self.username.clone(), self.user_id.clone()));
        let msg_rx = network.take_receiver();
        let broadcast_tx = network.broadcast_sender();
        network.start().await?;
        let announcer = crate::network::discovery::spawn_announcer(crate::config::port());
        self._discovery_handles.push(announcer);
        self.network = Some(network);
        self.msg_rx = msg_rx;
        self.broadcast_tx = Some(broadcast_tx);
        self.mode = AppMode::Chat;
        Ok(())
    }

    pub async fn connect_to(&mut self, addr: String) -> anyhow::Result<()> {
        let mut network = Box::new(Client::new(addr, self.username.clone(), self.user_id.clone()));
        let msg_rx = network.take_receiver();
        let broadcast_tx = network.broadcast_sender();
        network.start().await?;
        self.network = Some(network);
        self.msg_rx = msg_rx;
        self.broadcast_tx = Some(broadcast_tx);
        self.mode = AppMode::Chat;
        Ok(())
    }

    pub fn send_message(&mut self) -> bool {
        let content = self.input.trim().to_string();
        if content.is_empty() {
            return false;
        }
        self.input.clear();
        self.cursor = 0;

        if content.starts_with('/') {
            return self.handle_command(&content);
        }

        if let Some(tx) = &self.broadcast_tx {
            let msg = Message::Text {
                sender_id: self.user_id.clone(),
                sender_name: self.username.clone(),
                content,
            };
            let _ = tx.try_send(msg);
        }
        false
    }

    fn handle_command(&mut self, cmd: &str) -> bool {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        match parts[0] {
            "/quit" | "/q" => {
                self.quit = true;
                true
            }
            "/nick" | "/n" => {
                if let Some(new_name) = parts.get(1).filter(|n| !n.trim().is_empty()) {
                    let new_name = new_name.trim().to_string();
                    config::save(&new_name, &self.user_id);
                    self.username = new_name;
                    push_with_cap(
                        &mut self.messages,
                        ChatMessage {
                            time: Self::current_time(),
                            sender: "SYSTEM".into(),
                            sender_id: String::new(),
                            content: format!("Nickname changed to {}", self.username),
                        },
                    );
                } else {
                    push_with_cap(
                        &mut self.messages,
                        ChatMessage {
                            time: Self::current_time(),
                            sender: "SYSTEM".into(),
                            sender_id: String::new(),
                            content: "Usage: /nick <name>".into(),
                        },
                    );
                }
                false
            }
            "/list" | "/l" => {
                let names = if self.online_users.is_empty() {
                    vec!["(unknown, you are alone)"]
                } else {
                    self.online_users.iter().map(|s| s.as_str()).collect()
                };
                push_with_cap(
                    &mut self.messages,
                    ChatMessage {
                        time: Self::current_time(),
                        sender: "SYSTEM".into(),
                        sender_id: String::new(),
                        content: format!("Online: {}", names.join(", ")),
                    },
                );
                false
            }
            "/help" | "/h" => {
                push_with_cap(
                    &mut self.messages,
                    ChatMessage {
                        time: Self::current_time(),
                        sender: "SYSTEM".into(),
                        sender_id: String::new(),
                        content: "Commands: /quit, /nick <name>, /list, /help".into(),
                    },
                );
                false
            }
            _ => {
                push_with_cap(
                    &mut self.messages,
                    ChatMessage {
                        time: Self::current_time(),
                        sender: "SYSTEM".into(),
                        sender_id: String::new(),
                        content: format!("Unknown command: {}", parts[0]),
                    },
                );
                false
            }
        }
    }

    pub fn poll_messages(&mut self) {
        if let Some(rx) = &mut self.msg_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Message::Text { sender_id, sender_name, content } => {
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender: sender_name,
                                sender_id,
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
                                sender_id: String::new(),
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
                                sender_id: String::new(),
                                content: format!("{} left", peer.name),
                            },
                        );
                    }
                    Message::UserList { peers } => {
                        self.online_users = peers.iter().map(|p| p.name.clone()).collect();
                        let names: Vec<&str> = peers.iter().map(|p| p.name.as_str()).collect();
                        push_with_cap(
                            &mut self.messages,
                            ChatMessage {
                                time: Self::current_time(),
                                sender: "SYSTEM".into(),
                                sender_id: String::new(),
                                content: format!("Online: {}", names.join(", ")),
                            },
                        );
                    }
                }
            }
        }
    }

    const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

    pub fn poll_discovery(&mut self) {
        let rx = match &mut self.discovery_rx {
            Some(rx) => rx,
            None => return,
        };
        let now = std::time::Instant::now();
        if let AppMode::Menu(ref mut menu) = self.mode {
            while let Ok(addr) = rx.try_recv() {
                if let Some(entry) = menu.discovered_servers.iter_mut().find(|(a, _)| a == &addr) {
                    entry.1 = now;
                } else {
                    menu.discovered_servers.push((addr, now));
                    if menu.discovered_servers.len() > 20 {
                        menu.discovered_servers.remove(0);
                    }
                }
            }
            menu.discovered_servers
                .retain(|(_, last_seen)| now.duration_since(*last_seen) < Self::DISCOVERY_TIMEOUT);
        }
    }

    pub fn leave_chat(&mut self) {
        if let Some(tx) = &self.broadcast_tx {
            let msg = Message::Leave {
                peer: PeerInfo {
                    id: self.user_id.clone(),
                    name: self.username.clone(),
                },
            };
            let _ = tx.try_send(msg);
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

impl Drop for App {
    fn drop(&mut self) {
        self.save_history();
        for handle in &self._discovery_handles {
            handle.abort();
        }
    }
}

impl App {
    fn save_history(&self) {
        if self.messages.is_empty() {
            return;
        }
        let dir = config::config_dir_for_history();
        let _ = std::fs::create_dir_all(&dir);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = dir.join(format!("chat-{}.txt", ts));
        let mut content = String::new();
        for msg in &self.messages {
            let sender = if msg.sender == "SYSTEM" {
                "*".to_string()
            } else {
                msg.sender.clone()
            };
            content.push_str(&format!("[{}] {}: {}\n", msg.time, sender, msg.content));
        }
        let _ = std::fs::write(&path, content);
    }
}

fn push_with_cap(vec: &mut Vec<ChatMessage>, msg: ChatMessage) {
    if vec.len() >= MAX_MESSAGES {
        vec.remove(0);
    }
    vec.push(msg);
}
