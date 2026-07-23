use crate::app::{App, AppMode, MenuState};

use unicode_width::UnicodeWidthStr;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

fn cursor_x_for(input: &str, cursor: usize) -> u16 {
    let bound = if input.is_char_boundary(cursor) { cursor } else { input.len() };
    let visual = &input[..bound];
    visual.width() as u16
}

fn set_cursor(frame: &mut Frame, area: Rect, input: &str, cursor: usize, line: u16, prefix_width: u16) {
    let x = area.x + prefix_width + cursor_x_for(input, cursor);
    let y = area.y + line;
    frame.set_cursor_position((x, y));
}

pub fn render(frame: &mut Frame, app: &App) {
    match &app.mode {
        AppMode::Setup => render_setup(frame, &app.username, app.cursor),
        AppMode::Menu(menu) => {
            if menu.show_help {
                render_help(frame);
            } else if menu.show_settings {
                render_settings(frame, menu, &app.username);
            } else if menu.show_input {
                render_menu(frame, menu);
                let inner = menu_input_area(frame.area());
                set_cursor(frame, inner, &menu.server_addr, menu.server_cursor, 6, 11);
            } else {
                render_menu(frame, menu);
            }
        }
        AppMode::Chat => render_chat(frame, app),
    }
}

fn menu_input_area(area: Rect) -> Rect {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(3),
            Constraint::Percentage(40),
        ])
        .split(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Percentage(20)])
                .split(area)[1],
        );
    inner[1]
}

fn render_setup(frame: &mut Frame, username: &str, cursor: usize) {
    let area = frame.area();

    let block = Block::default().borders(Borders::ALL).title(" Welcome ");
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(3),
            Constraint::Percentage(40),
        ])
        .split(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Percentage(20)])
                .split(area)[1],
        );

    let lines = vec![
        Line::from(Span::styled(
            " Welcome to Lan Chat!",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(" Please set your nickname:"),
        Line::from(""),
        Line::from(vec![
            Span::styled(" Name: ", Style::default().fg(Color::Yellow)),
            Span::raw(username),
        ]),
    ];

    let widget = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(widget, inner[1]);

    let hint = Paragraph::new(" Enter: Confirm  |  Ctrl+C / Esc: Quit ")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint, inner[2]);

    let label_width = " Name: ".width() as u16;
    let bound = if username.is_char_boundary(cursor) { cursor } else { username.len() };
    let username_prefix = &username[..bound];
    let username_width = username_prefix.width() as u16;
    let full_width = label_width + username[..].width() as u16;
    let cx = inner[1].x + (inner[1].width.saturating_sub(full_width)) / 2 + label_width + username_width;
    let cy = inner[1].y + 4;
    frame.set_cursor_position((cx, cy));
}

fn render_menu(frame: &mut Frame, menu: &MenuState) {
    let area = frame.area();

    let block = Block::default().borders(Borders::ALL).title(" Lan Chat ");
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(3),
            Constraint::Percentage(40),
        ])
        .split(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Percentage(20)])
                .split(area)[1],
        );

    // title
    let title = Paragraph::new("Lan Chat")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, inner[0]);

    // options
    let mut lines = vec![
        Line::from(Span::styled(
            "  [1] Start Server",
            if !menu.show_input {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  [2] Connect to Server",
            if menu.show_input {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  [3] Settings",
            Style::default().fg(Color::Magenta),
        )),
    ];
    if menu.show_input {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  Address: ", Style::default().fg(Color::Cyan)),
            Span::raw(&menu.server_addr),
        ]));
    }
    if let Some(err) = &menu.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  Error: {}", err),
            Style::default().fg(Color::Red),
        )));
    }
    if menu.connecting {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Connecting...",
            Style::default().fg(Color::Yellow),
        )));
    }

    if !menu.show_input && !menu.discovered_servers.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  ── LAN ──",
            Style::default().fg(Color::DarkGray),
        )));
        for (i, (addr, _)) in menu.discovered_servers.iter().enumerate() {
            let key = i + 4;
            if key <= 9 {
                lines.push(Line::from(Span::styled(
                    format!("  [{}] {}", key, addr),
                    Style::default().fg(Color::Green),
                )));
            } else {
                break;
            }
        }
    }

    let options = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(options, inner[1]);

    // hint
    let hint_text = if menu.show_input {
        " Esc: Back  |  Enter: Connect "
    } else if !menu.discovered_servers.is_empty() {
        " Esc/q: Quit  |  1: Server  2: Connect  3: Settings  4-9: LAN  h: Help "
    } else {
        " Esc/q: Quit  |  1: Server  2: Connect  3: Settings  h: Help  Enter: Server "
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint, inner[2]);
}

fn render_help(frame: &mut Frame) {
    let area = frame.area();

    let block = Block::default().borders(Borders::ALL).title(" Help ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = vec![
        Line::from(Span::styled(" Lan Chat - 终端聊天室", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(" 本机启动服务端，其他人用客户端连接即可聊天。"),
        Line::from(""),
        Line::from(Span::styled(" ── 启动 ──", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from(" 服务端（本机）："),
        Line::from("   [1] 启动服务端 → 自动广播到局域网"),
        Line::from("   或命令行：cargo run -- --server"),
        Line::from(""),
        Line::from(" 客户端（他人）："),
        Line::from("   [2] 输入服务端的 IP:端口 → 回车"),
        Line::from("   或按 [3] [4] ... 连接自动发现的服务器"),
        Line::from("   或命令行：cargo run -- --connect IP:端口"),
        Line::from(""),
        Line::from(Span::styled(" ── 聊天操作 ──", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from("   字母/数字/空格   输入文字"),
        Line::from("   Backspace        删除"),
        Line::from("   Enter            发送"),
        Line::from("   Ctrl+C / Esc / q 退出"),
        Line::from(""),
        Line::from(Span::styled(" ── 命令行参数 ──", Style::default().fg(Color::Yellow))),
        Line::from(""),
        Line::from("   --server / -s              跳过菜单，启动服务端"),
        Line::from("   --connect <ip:port>        跳过菜单，直接连接"),
        Line::from("   -p / --port <number>       指定端口（默认 9876）"),
        Line::from("   -n / --name <昵称>          自定义昵称"),
        Line::from(""),
        Line::from(Span::styled(" 按任意键返回菜单", Style::default().fg(Color::DarkGray))),
    ];

    let help_widget = Paragraph::new(text);
    frame.render_widget(help_widget, inner);
}

fn render_settings(frame: &mut Frame, menu: &MenuState, username: &str) {
    let area = frame.area();

    let block = Block::default().borders(Borders::ALL).title(" Settings ");
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Min(3),
            Constraint::Percentage(40),
        ])
        .split(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(20), Constraint::Percentage(60), Constraint::Percentage(20)])
                .split(area)[1],
        );

    let mut lines = vec![
        Line::from(Span::styled(
            "  [1] Nickname",
            if !menu.edit_username {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Current: ", Style::default().fg(Color::DarkGray)),
            Span::styled(username, Style::default().fg(Color::Cyan)),
        ]),
    ];
    if menu.edit_username {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  New: ", Style::default().fg(Color::Cyan)),
            Span::raw(&menu.username_input),
        ]));
    }

    let options = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(options, inner[1]);

    let hint_text = if menu.edit_username {
        " Esc: Back  |  Enter: Save "
    } else {
        " Esc: Back  |  1: Edit Nickname "
    };
    let hint = Paragraph::new(hint_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(hint, inner[2]);

    if menu.edit_username {
        let prefix = "  New: ";
        let prefix_width = prefix.width() as u16;
        let bound = if menu.username_input.is_char_boundary(menu.username_cursor) { menu.username_cursor } else { menu.username_input.len() };
        let input_prefix = &menu.username_input[..bound];
        let input_width = input_prefix.width() as u16;
        let cx = inner[1].x + prefix_width + input_width;
        let cy = inner[1].y + 4;
        frame.set_cursor_position((cx, cy));
    }
}

fn render_chat(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let main_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Lan Chat - {} ", app.username));
    let inner = main_block.inner(area);
    frame.render_widget(main_block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(inner);

    // status bar
    let status = format!(" {} ", app.status_line());
    let status_style = if status.contains("Server") {
        Style::default().fg(Color::Yellow)
    } else if status.contains("Connected") {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };
    let status_widget = Paragraph::new(Line::from(Span::styled(status, status_style)));
    frame.render_widget(status_widget, chunks[0]);

    // message list
    let msg_area = chunks[1];
    let msg_height = (msg_area.height.saturating_sub(2)) as usize;
    let start = if app.messages.len() > msg_height {
        app.messages.len() - msg_height
    } else {
        0
    };

    let lines: Vec<Line> = app.messages[start..]
        .iter()
        .map(|m| {
            let style = if m.sender == "SYSTEM" {
                Style::default().fg(Color::DarkGray)
            } else if m.sender_id == app.user_id {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Green)
            };
            Line::from(vec![
                Span::styled(format!("[{}] ", m.time), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}: ", m.sender), style),
                Span::raw(&m.content),
            ])
        })
        .collect();

    let msg_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Messages ({}) ", app.messages.len())),
    );
    frame.render_widget(msg_widget, msg_area);

    // input
    let input_widget = Paragraph::new(app.input.as_str()).block(
        Block::default().borders(Borders::ALL).title(" Input "),
    );
    frame.render_widget(input_widget, chunks[2]);

    // cursor
    let bound = if app.input.is_char_boundary(app.cursor) { app.cursor } else { app.input.len() };
    let input_visual = &app.input[..bound];
    let visual_x = input_visual.width();
    let x = chunks[2].x + 1 + visual_x as u16;
    let y = chunks[2].y + 1;
    frame.set_cursor_position((x, y));
}


