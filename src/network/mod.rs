use crate::protocol::Message;
use async_trait::async_trait;

#[async_trait]
pub trait Network: Send {
    async fn start(&mut self) -> anyhow::Result<()>;
    fn take_receiver(&mut self) -> Option<tokio::sync::mpsc::UnboundedReceiver<Message>>;
    fn broadcast_sender(&self) -> tokio::sync::mpsc::UnboundedSender<Message>;
    fn status_line(&self) -> String;
}

mod server;
mod client;
pub mod discovery;

pub use server::Server;
pub use client::Client;
