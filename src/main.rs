mod app;
mod config;
mod network;
mod protocol;
mod ui;

use crate::app::{App, AppMode};

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
                    config::set_server_addr(addr);
                }
            }
            "-s" | "--server" => {
                config::set_server_mode();
            }
            _ => {}
        }
    }
}

async fn async_main() -> anyhow::Result<()> {
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

    let username = whoami::username();
    let mut app = App::new(username);

    // CLI overrides: skip menu
    if config::is_server() {
        app.start_server().await.context("failed to start server")?;
    } else if let Some(addr) = config::server_addr() {
        app.connect_to(addr.to_string()).await.context("failed to connect")?;
    }

    let result = run_event_loop(&mut terminal, &mut app).await;

    disable_raw_mode().context("disable_raw_mode failed")?;
    execute!(io::stdout(), LeaveAlternateScreen).context("leave alt screen failed")?;

    result
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
                    AppMode::Menu(_) => handle_menu_event(app, event).await,
                    AppMode::Chat => handle_chat_event(app, event).await,
                };
                if quit {
                    break;
                }
            }
            _ = tick.tick() => {
                if matches!(app.mode, AppMode::Chat) {
                    app.poll_messages();
                }
            }
        }

        if app.quit {
            break;
        }
    }

    Ok(())
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
        let is_inputting = matches!(&app.mode, AppMode::Menu(m) if m.show_input);

        // In input mode: all chars go to address buffer
        if is_inputting {
            match key.code {
                KeyCode::Esc => {
                    // Cancel input, back to menu
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.show_input = false;
                        menu.server_addr.clear();
                    }
                }
                KeyCode::Char('q') => return true,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
                KeyCode::Enter => {
                    let addr = match &app.mode {
                        AppMode::Menu(m) => m.server_addr.trim().to_string(),
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
                        menu.server_addr.pop();
                    }
                }
                KeyCode::Char(c) if c.is_ascii_graphic() || c == ' ' || c == ':' || c == '.' => {
                    if let AppMode::Menu(ref mut menu) = app.mode {
                        menu.server_addr.push(c);
                    }
                }
                _ => {}
            }
            return false;
        }

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

            KeyCode::Enter => {
                let _ = app.start_server().await;
            }

            _ => {}
        }
    }
    false
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
                if c.is_ascii_graphic() || c == ' ' {
                    app.input.push(c);
                }
            }
            KeyCode::Backspace => {
                app.input.pop();
            }
            KeyCode::Enter => {
                app.send_message();
            }
            _ => {}
        }
    }
    false
}
