use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Paragraph},
};
use std::io;

const COMPONENT_NAMES: [&str; 3] = ["nav", "cup", "ram"];
const COMPONENT_DESCRIPTIONS: [&str; 3] = [
    "the intelligent cd replacement in Rust",
    "faster cp command",
    "safer rm command",
];
const CRATE_NAMES: [&str; 3] = ["navcli", "cupcli", "ramcli"];

#[derive(Clone, Copy, PartialEq)]
enum Method {
    CargoInstall,
    Curl,
}

struct State {
    selected: [bool; 3],
    method: Method,
    cursor: usize,
    confirmed: bool,
}

impl State {
    fn new() -> Self {
        Self {
            selected: [false; 3],
            method: Method::CargoInstall,
            cursor: 0,
            confirmed: false,
        }
    }

    fn all_selected(&self) -> bool {
        self.selected.iter().all(|&s| s)
    }

    fn toggle_cursor(&mut self) {
        if self.cursor < 4 {
            if self.cursor < 3 {
                self.selected[self.cursor] = !self.selected[self.cursor];
            } else {
                let val = !self.all_selected();
                self.selected = [val; 3];
            }
        }
    }

    fn toggle_method(&mut self) {
        self.method = match self.method {
            Method::CargoInstall => Method::Curl,
            Method::Curl => Method::CargoInstall,
        };
    }

    fn selected_count(&self) -> usize {
        self.selected.iter().filter(|&&s| s).count()
    }

    fn selected_crates(&self) -> Vec<&'static str> {
        self.selected
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s)
            .map(|(i, _)| CRATE_NAMES[i])
            .collect()
    }

    fn selected_names(&self) -> Vec<&'static str> {
        self.selected
            .iter()
            .enumerate()
            .filter(|&(_, &s)| s)
            .map(|(i, _)| COMPONENT_NAMES[i])
            .collect()
    }
}

pub fn run() {
    if let Err(e) = run_inner() {
        eprintln!("Error: {}", e);
    }
}

fn run_inner() -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = State::new();
    let res = run_app(&mut terminal, &mut state);

    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    if let Ok(()) = res {
        if state.confirmed && state.selected_count() > 0 {
            let names = state.selected_names();
            print_instructions(&state.method, &names, &state.selected_crates());
        } else if state.confirmed {
            println!("No components selected.");
        }
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut State,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, state))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                KeyCode::Enter => {
                    state.confirmed = true;
                    break Ok(());
                }
                KeyCode::Char(' ') => {
                    state.toggle_cursor();
                }
                KeyCode::Tab | KeyCode::Char('m') => {
                    state.toggle_method();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.cursor = state.cursor.saturating_sub(1);
                }
                KeyCode::Down | KeyCode::Char('j') if state.cursor < 4 => {
                    state.cursor += 1;
                }
                _ => {}
            }
        }
    }
}

fn ui(frame: &mut Frame, state: &State) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Length(3),
        ])
        .split(area);

    let title = Paragraph::new("Neoutils Toolkit Installer")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    frame.render_widget(title, layout[0]);

    let method_display = Line::from(vec![
        Span::styled(
            if state.method == Method::CargoInstall {
                " ● cargo install"
            } else {
                "   cargo install"
            },
            if state.method == Method::CargoInstall {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw("  "),
        Span::styled(
            if state.method == Method::Curl {
                " ● curl"
            } else {
                "   curl"
            },
            if state.method == Method::Curl {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::raw("    [Tab] toggle"),
    ]);
    let method = Paragraph::new(method_display)
        .block(
            Block::default()
                .title(" Install Method ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .alignment(Alignment::Left);
    frame.render_widget(method, layout[1]);

    let mut items = Vec::new();
    for i in 0..3 {
        let checked = if state.selected[i] { "☑" } else { "☐" };
        let prefix = if i == state.cursor { "▸ " } else { "  " };
        let style = if i == state.cursor {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        items.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{} {}  ", checked, COMPONENT_NAMES[i]), style),
            Span::styled(
                COMPONENT_DESCRIPTIONS[i],
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }
    {
        let i = 3;
        let checked = if state.all_selected() { "☑" } else { "☐" };
        let prefix = if i == state.cursor { "▸ " } else { "  " };
        let style = if i == state.cursor {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        items.push(Line::from(vec![
            Span::styled(prefix, style),
            Span::styled(format!("{} all", checked), style),
            Span::styled("  install everything", Style::default().fg(Color::DarkGray)),
        ]));
    }

    let comp_block = Paragraph::new(Text::from(items))
        .block(
            Block::default()
                .title(" Components ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .alignment(Alignment::Left);
    frame.render_widget(comp_block, layout[2]);

    let footer = Paragraph::new("[Space] Toggle  [Enter] Confirm  [Tab] Method  [q] Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        );
    frame.render_widget(footer, layout[3]);
}

fn print_instructions(method: &Method, names: &[&str], crates: &[&str]) {
    println!();
    println!("{}", "=".repeat(50));
    println!("  Neoutils Toolkit - Install Instructions");
    println!("{}", "=".repeat(50));
    println!();
    println!("  Selected: {}", names.join(", "));
    println!();
    match method {
        Method::CargoInstall => {
            println!("  Run:");
            println!();
            println!("    cargo install {}", crates.join(" "));
        }
        Method::Curl => {
            println!("  Run:");
            println!();
            println!(
                "    curl -sSL https://neoutils.dev/install.sh | bash -s -- --components {}",
                names.join(",")
            );
        }
    }
    println!();
}
