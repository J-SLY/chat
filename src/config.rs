use std::sync::OnceLock;

pub const DEFAULT_PORT: u16 = 9876;

static SERVER_PORT: OnceLock<u16> = OnceLock::new();
static SERVER_ADDR: OnceLock<String> = OnceLock::new();
static IS_SERVER: OnceLock<bool> = OnceLock::new();
static USER_NAME: OnceLock<String> = OnceLock::new();

pub fn set_port(port: u16) {
    let _ = SERVER_PORT.set(port);
}

pub fn port() -> u16 {
    *SERVER_PORT.get().unwrap_or(&DEFAULT_PORT)
}

pub fn set_server_addr(addr: String) {
    let _ = SERVER_ADDR.set(addr);
}

pub fn server_addr() -> Option<&'static str> {
    SERVER_ADDR.get().map(String::as_str)
}

pub fn set_server_mode() {
    let _ = IS_SERVER.set(true);
}

pub fn is_server() -> bool {
    *IS_SERVER.get().unwrap_or(&false)
}

pub fn set_name(name: String) {
    let _ = USER_NAME.set(name);
}

pub fn name() -> Option<&'static str> {
    USER_NAME.get().map(String::as_str)
}
