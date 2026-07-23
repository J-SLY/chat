use std::path::PathBuf;
use std::sync::OnceLock;

pub const DEFAULT_PORT: u16 = 9876;
const CONFIG_DIR: &str = "rust-chat";
const CONFIG_FILE: &str = "config.toml";

static SERVER_PORT: OnceLock<u16> = OnceLock::new();
static SERVER_ADDR: OnceLock<String> = OnceLock::new();
static IS_SERVER: OnceLock<bool> = OnceLock::new();
static USER_NAME: OnceLock<String> = OnceLock::new();
static USER_ID: OnceLock<String> = OnceLock::new();

fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_DIR)
}

fn config_file() -> PathBuf {
    config_dir().join(CONFIG_FILE)
}

fn load_config_raw() -> Option<String> {
    let path = config_file();
    if path.exists() {
        std::fs::read_to_string(&path).ok()
    } else {
        None
    }
}

fn load_config_username() -> Option<String> {
    load_config_raw().and_then(|s| {
        toml::from_str::<Config>(&s).ok().and_then(|c| c.username)
    })
}

fn load_config_user_id() -> Option<String> {
    load_config_raw().and_then(|s| {
        toml::from_str::<Config>(&s).ok().and_then(|c| c.user_id)
    })
}

fn save_config(name: &str, id: &str) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let config = Config {
        username: Some(name.to_string()),
        user_id: Some(id.to_string()),
    };
    let _ = std::fs::write(config_file(), toml::to_string(&config).unwrap_or_default());
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Config {
    username: Option<String>,
    user_id: Option<String>,
}

pub fn has_config() -> bool {
    config_file().exists()
}

pub fn generate_user_id(name: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, name.as_bytes())
        .to_string()
}

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

pub fn name() -> Option<&'static str> {
    USER_NAME.get().map(String::as_str)
}

pub fn set_user_id(id: String) {
    let _ = USER_ID.set(id);
}

pub fn user_id() -> Option<&'static str> {
    USER_ID.get().map(String::as_str)
}

pub fn save(name: &str, id: &str) {
    save_config(name, id);
    let _ = USER_NAME.set(name.to_string());
    let _ = USER_ID.set(id.to_string());
}

pub fn load_saved_name() -> Option<String> {
    load_config_username()
}

pub fn load_saved_user_id() -> Option<String> {
    load_config_user_id()
}

pub fn config_dir_for_history() -> PathBuf {
    config_dir().join("history")
}

pub fn config_dir_for_server_logs() -> PathBuf {
    config_dir().join("server-logs")
}
