mod app;
mod config;
mod network;
mod protocol;
mod ui;

use crate::app::{App, AppMode, MenuState};

use anyhow::Context;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::panic;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    parse_args();

    let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
    rt.block_on(async_main())
}

fn parse_args() {
    let mut args = std::env::args().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--port" => {
                if let Some(port) = args.next() {
                    if let Ok(port) = port.parse::<u16>() {
                        config::set_port(port);
                    }
                }
            }
            "-c" | "--connect" => {
                if let Some(addr) = args.next() {
                    let addr = if !addr.contains(':') {
                        format!("{}:{}", addr, config::port())
                    } else {
                        addr
                    };
                    config::set_server_addr(addr);
                }
            }
            "-s" | "--server" => {
                config::set_server_mode();
            }
            "-n" | "--name" => {
                if let Some(name) = args.next() {
                    let user_id = config::generate_user_id(&name);
                    config::save(&name, &user_id);
                }
            }
            _ => {}
        }
    }
}

async fn async_main() -> anyhow::Result<()> {
    let (name, user_id) = if config::name().is_some() {
        let name = config::name().unwrap().to_string();
        let user_id = config::user_id()
            .map(|s| s.to_string())
            .unwrap_or_else(|| config::generate_user_id(&name));
        (name, user_id)
    } else if let Some(saved_name) = config::load_saved_name() {
        let user_id = config::load_saved_user_id()
            .unwrap_or_else(|| config::generate_user_id(&saved_name));
        (saved_name, user_id)
    } else {
        let name = whoami::username();
        let user_id = config::generate_user_id(&name);
        (name, user_id)
    };

    config::set_user_id(user_id.clone());

    let mut app = App::new(name, user_id);

    enable_raw_mode().context("enable_raw_mode failed")?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alt screen failed")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("terminal init failed")?;

    let panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        panic_hook(panic);
    }));

    // First launch: show setup before CLI overrides
    if !config::has_config() && config::name().is_none() {
        app.mode = AppMode::Setup;
    }

    // CLI overrides: skip menu
    if config::is_server() {
        app.start_server().await.context("failed to start server")?;
    } else if let Some(addr) = config::server_addr() {
        app.connect_to(addr.to_string()).await.context("failed to connect")?;
    }

    let _ = run_event_loop(&mut terminal, &mut app).await;

    disable_raw_mode().context("disable_raw_mode failed")?;
    execute!(io::stdout(), LeaveAlternateScreen).context("leave alt screen failed")?;

    Ok(())
}

async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    let mut reader = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        terminal.draw(|f| ui::render(f, app))?;

        tokio::select! {
            Some(Ok(event)) = reader.next() => {
                let quit = match &app.mode {
                    AppMode::Setup => handle_setup_event(app, event).await,
                    AppMode::Menu(_) => handle_menu_event(app, event).await,
                    AppMode::Chat => handle_chat_event(app, event).await,
                };
                if quit {
                    if matches!(app.mode, AppMode::Chat) {
                        app.leave_chat();
                    }
                    break;
                }
            }
            _ = tick.tick() => {
                match app.mode {
                    AppMode::Chat => app.poll_messages(),
                    AppMode::Menu(_) => app.poll_discovery(),
                    AppMode::Setup => {}
                }
            }
        }

        if app.quit {
            if matches!(app.mode, AppMode::Chat) {
                app.leave_chat();
            }
            break;
        }
    }

    Ok(())
}

async fn handle_setup_event(app: &mut App, event: Event) -> bool {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                app.quit = true;
                true
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.quit = true;
                true
            }
            KeyCode::Enter => {
                let name = app.username.trim().to_string();
                if !name.is_empty() {
                    let user_id = config::generate_user_id(&name);
                    config::save(&name, &user_id);
                    app.username = name;
                    app.user_id = user_id;
                    app.cursor = 0;
                    app.mode = AppMode::Menu(MenuState {
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
                    });
                }
                false
            }
            KeyCode::Left => {
                if app.cursor > 0 {
                    let mut p = app.cursor.saturating_sub(1);
                    while !app.username.is_char_boundary(p) {
                        p = p.saturating_sub(1);
                    }
                    app.cursor = p;
                }
                false
            }
            KeyCode::Right => {
                let mut p = (app.cursor + 1).min(app.username.len());
                while !app.username.is_char_boundary(p) {
                    p += 1;
                }
                app.cursor = p.min(app.username.len());
                false
            }
            KeyCode::Backspace => {
                if app.cursor > 0 {
                    let prev = app.username[..app.cursor].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                    app.username.remove(prev);
                    app.cursor = prev;
                }
                false
            }
            KeyCode::Char(c) => {
                app.username.insert(app.cursor, c);
                app.cursor += c.len_utf8();
                false
            }
            _ => false,
        }
    } else {
        false
    }
}

async fn handle_menu_event(app: &mut App, event: Event) -> bool {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        // In help mode: most keys go back
        if matches!(&app.mode, AppMode::Menu(m) if m.show_help) {
            return match key.code {
                KeyCode::Char('q') | KeyCode::Esc => true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
                _ => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.show_help = false;
                    }
                    false
                }
            };
        }
        // In settings mode
        if matches!(&app.mode, AppMode::Menu(m) if m.show_settings) {
            if matches!(&app.mode, AppMode::Menu(m) if m.edit_username) {
                return handle_settings_username_input(app, event);
            }
            return match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.show_settings = false;
                    }
                    false
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
                KeyCode::Char('1') => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.edit_username = true;
                        menu.username_input = app.username.clone();
                        menu.username_cursor = menu.username_input.len();
                    }
                    false
                }
                _ => false,
            };
        }
        let is_inputting = matches!(&app.mode, AppMode::Menu(m) if m.show_input);

        // In input mode: all chars go to address buffer
        if is_inputting {
            match key.code {
                KeyCode::Esc => {
                    // Cancel input, back to menu
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.show_input = false;
                        menu.server_addr.clear();
                        menu.server_cursor = 0;
                    }
                }
                KeyCode::Char('q') => return true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
                KeyCode::Enter => {
                    let addr = match &app.mode {
                        AppMode::Menu(m) => {
                            let addr = m.server_addr.trim().to_string();
                            if !addr.contains(':') {
                                format!("{}:{}", addr, crate::config::port())
                            } else {
                                addr
                            }
                        }
                        _ => String::new(),
                    };
                    if !addr.is_empty() {
                        if let AppMode::Menu(ref mut m) = app.mode {
                            m.connecting = true;
                        }
                        let result = app.connect_to(addr).await;
                        if let Err(e) = result {
                            if let AppMode::Menu(ref mut m) = app.mode {
                                m.connecting = false;
                                m.error = Some(e.to_string());
                            }
                        }
                    }
                }
                KeyCode::Backspace => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        let pos = menu.server_cursor;
                        if pos > 0 {
                            let prev = menu.server_addr[..pos].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                            menu.server_addr.remove(prev);
                            menu.server_cursor = prev;
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.server_addr.insert(menu.server_cursor, c);
                        menu.server_cursor += c.len_utf8();
                    }
                }
                KeyCode::Left => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        if menu.server_cursor > 0 {
                            let mut p = menu.server_cursor.saturating_sub(1);
                            while !menu.server_addr.is_char_boundary(p) {
                                p = p.saturating_sub(1);
                            }
                            menu.server_cursor = p;
                        }
                    }
                }
                KeyCode::Right => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        let mut p = (menu.server_cursor + 1).min(menu.server_addr.len());
                        while !menu.server_addr.is_char_boundary(p) {
                            p += 1;
                        }
                        menu.server_cursor = p.min(menu.server_addr.len());
                    }
                }
                _ => {}
            }
            return false;
        }

        let discovered_count = match &app.mode {
            AppMode::Menu(m) => m.discovered_servers.len(),
            _ => 0,
        };

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,

            KeyCode::Char('1') => {
                let _ = app.start_server().await;
            }

            KeyCode::Char('2') => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.show_input = true;
                }
            }

            KeyCode::Char('h') | KeyCode::Char('?') => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.show_help = !menu.show_help;
                }
            }

            KeyCode::Char('3') => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.show_settings = true;
                    menu.edit_username = false;
                }
            }

            KeyCode::Char(c) if c.is_ascii_digit() && ('4'..='9').contains(&c) => {
                let idx = (c as u8 - b'0') as usize - 4;
                if idx < discovered_count {
                    let addr = match &app.mode {
                        AppMode::Menu(m) => m.discovered_servers.get(idx).map(|(a, _)| a.clone()).unwrap_or_default(),
                        _ => String::new(),
                    };
                    if !addr.is_empty() {
                        if let AppMode::Menu(ref mut m) = app.mode {
                            m.connecting = true;
                        }
                        let result = app.connect_to(addr).await;
                        if let Err(e) = result {
                            if let AppMode::Menu(ref mut m) = app.mode {
                                m.connecting = false;
                                m.error = Some(e.to_string());
                            }
                        }
                    }
                }
            }

            KeyCode::Enter => {
                let _ = app.start_server().await;
            }

            _ => {}
        }
    }
    false
}

fn handle_settings_username_input(app: &mut App, event: Event) -> bool {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Esc => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.edit_username = false;
                    menu.username_input.clear();
                    menu.username_cursor = 0;
                }
                false
            }
            KeyCode::Char('q') => true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
            KeyCode::Enter => {
                let new_name = match &app.mode {
                    AppMode::Menu(m) => m.username_input.trim().to_string(),
                    _ => String::new(),
                };
                if !new_name.is_empty() {
                    config::save(&new_name, &app.user_id);
                    app.username = new_name;
                }
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.edit_username = false;
                    menu.username_input.clear();
                    menu.username_cursor = 0;
                }
                false
            }
            KeyCode::Backspace => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    let pos = menu.username_cursor;
                    if pos > 0 {
                        let prev = menu.username_input[..pos].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                        menu.username_input.remove(prev);
                        menu.username_cursor = prev;
                    }
                }
                false
            }
            KeyCode::Char(c) => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    menu.username_input.insert(menu.username_cursor, c);
                    menu.username_cursor += c.len_utf8();
                }
                false
            }
            KeyCode::Left => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    if menu.username_cursor > 0 {
                        let mut p = menu.username_cursor.saturating_sub(1);
                        while !menu.username_input.is_char_boundary(p) {
                            p = p.saturating_sub(1);
                        }
                        menu.username_cursor = p;
                    }
                }
                false
            }
            KeyCode::Right => {
                if let AppMode::Menu(ref mut menu) = app.mode {
                    let mut p = (menu.username_cursor + 1).min(menu.username_input.len());
                    while !menu.username_input.is_char_boundary(p) {
                        p += 1;
                    }
                    menu.username_cursor = p.min(menu.username_input.len());
                }
                false
            }
            _ => false,
        }
    } else {
        false
    }
}

async fn handle_chat_event(app: &mut App, event: Event) -> bool {
    if let Event::Key(key) = event {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('q') => return true,
            KeyCode::Esc => return true,
            KeyCode::Char(c) => {
                app.input.insert(app.cursor, c);
                app.cursor += c.len_utf8();
            }
            KeyCode::Backspace => {
                if app.cursor > 0 {
                    let prev = app.input[..app.cursor].char_indices().last().map(|(i, _)| i).unwrap_or(0);
                    app.input.remove(prev);
                    app.cursor = prev;
                }
            }
            KeyCode::Left => {
                if app.cursor > 0 {
                    let mut p = app.cursor.saturating_sub(1);
                    while !app.input.is_char_boundary(p) {
                        p = p.saturating_sub(1);
                    }
                    app.cursor = p;
                }
            }
            KeyCode::Right => {
                let mut p = (app.cursor + 1).min(app.input.len());
                while !app.input.is_char_boundary(p) {
                    p += 1;
                }
                app.cursor = p.min(app.input.len());
            }
            KeyCode::Enter if app.send_message() => return true,
            _ => {}
        }
    }
    false
}
