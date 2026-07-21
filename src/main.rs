use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, poll}, execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal, backend::{Backend, CrosstermBackend}, layout::{Constraint, Direction, Layout}, widgets::{Block, Borders, Paragraph, block::title},
};
use std::{io, panic, time::Duration};

struct App {
    text: String,
    quit: bool,
}

impl App {
    fn new() -> Self {
        Self {
            text: String::new(),
            quit: false,
        }
    }
}

fn main() -> io::Result<()> {
    //设置终端
    //enable_raw_mode()?;  // raw mode 会自动禁用回显
    
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    //崩溃时恢复终端 
    let panic_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        panic_hook(panic);
    }));

    let mut app = App::new();
    let result = run_app(&mut terminal, &mut app);

    //恢复终端
    disable_raw_mode()?;
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    
    result
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|f| ui(f, app))?;
        
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('c')if key.modifiers.contains(event::KeyModifiers::CONTROL)=>{
                    app.quit = true;
                }
                KeyCode::Char('q') => {
                    app.quit = true;
                }
                KeyCode::Char(c) => {
                    // 只添加可见字符
                    if c.is_ascii_graphic() || c == ' ' {
                        app.text.push(c);
                    }
                }
                KeyCode::Backspace => {
                    app.text.pop();
                }
                KeyCode::Enter => {
                    app.text.push('\n');
                }
                KeyCode::Esc => {
                    app.quit = true;
                }
                _ => {}
            }
        }
        
        
    }
    Ok(())
}

fn ui(frame: &mut Frame, app: &App) {
    let area = frame.area();
    
    let main_block = Block::default()
        .borders(Borders::ALL)
        .title("Chat TUI");
    
    let inner_area = main_block.inner(area);
    frame.render_widget(main_block, area);


    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70),
            Constraint::Percentage(30)
        ])
        .split(inner_area);

    let title = Paragraph::new("Chat TUI")
        .block(
            Block::default()
                .borders(Borders::ALL)
        );
    frame.render_widget(title, chunks[0]);

    let value = Paragraph::new(app.text.as_str())
        .block(
            Block::default()
                .borders(Borders::ALL)
        );
    frame.render_widget(value, chunks[1]);
}