use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{self, Duration};

const MULTICAST_ADDR: &str = "239.255.0.1";
const DISCOVERY_PORT: u16 = 9877;

#[derive(Serialize, Deserialize)]
struct Announcement {
    port: u16,
}

fn bind_reusable(addr: &str) -> std::io::Result<std::net::UdpSocket> {
    let sock = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    sock.set_reuse_address(true)?;
    let address: std::net::SocketAddr = addr.parse().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, e)
    })?;
    sock.bind(&address.into())?;
    sock.set_nonblocking(true)?;
    Ok(sock.into())
}

/// Listen for UDP multicast announcements from LAN servers.
/// Sends discovered "ip:port" strings into the given channel.
pub fn spawn_listener(tx: mpsc::Sender<String>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let std_socket = match bind_reusable(&format!("0.0.0.0:{}", DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let socket = match UdpSocket::from_std(std_socket) {
            Ok(s) => s,
            Err(_) => return,
        };
        let multicast: std::net::Ipv4Addr = match MULTICAST_ADDR.parse() {
            Ok(a) => a,
            Err(_) => return,
        };
        let interface: std::net::Ipv4Addr = "0.0.0.0".parse().expect("static address");
        let _ = socket.join_multicast_v4(multicast, interface);
        let mut buf = [0u8; 1024];
        while let Ok((len, src)) = socket.recv_from(&mut buf).await {
            if let Ok(ann) = serde_json::from_slice::<Announcement>(&buf[..len]) {
                let addr = format!("{}:{}", src.ip(), ann.port);
                let _ = tx.try_send(addr);
            }
        }
    })
}

/// Periodically announce this server on the LAN via UDP multicast.
pub fn spawn_announcer(port: u16) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0").await else {
            return;
        };
        let dest: std::net::SocketAddr = match format!("{}:{}", MULTICAST_ADDR, DISCOVERY_PORT).parse()
        {
            Ok(a) => a,
            Err(_) => return,
        };
        let ann = Announcement { port };
        let bytes = match serde_json::to_vec(&ann) {
            Ok(b) => b,
            Err(_) => return,
        };
        let mut interval = time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            let _ = socket.send_to(&bytes, dest).await;
        }
    })
}
