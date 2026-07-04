use std::{
    cmp::min,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use color_eyre::{Result, eyre::eyre};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    DefaultTerminal, Frame,
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, HighlightSpacing, List, ListItem, ListState, Paragraph, Row,
        Table, TableState, Widget, Wrap,
    },
};
use rusqlite::{Connection, params};

use self::ssh::{SshLaunch, launch_ssh, ssh_command, ssh_launch};

mod ssh;
mod theme;

const APP_NAME: &str = "russhx";
const DB_FILE: &str = "russhx.db";
const FORM_LABEL_WIDTH: usize = 17;
const FORM_INPUT_COL: u16 = 2 + FORM_LABEL_WIDTH as u16;

#[derive(Debug, Clone)]
struct Server {
    id: i64,
    name: String,
    host: String,
    username: String,
    port: i64,
    group_name: String,
    auth_type: String,
    password: Option<String>,
    key_path: Option<String>,
    key_passphrase: Option<String>,
    notes: String,
    tags: String,
    created_at: String,
    updated_at: String,
    last_used: Option<String>,
    favorite: bool,
}

#[derive(Debug, Clone)]
struct Group {
    id: i64,
    name: String,
    color: String,
    icon: String,
}

#[derive(Debug, Clone)]
struct Settings {
    theme: String,
    default_port: i64,
    last_selected_server: Option<i64>,
    confirm_before_delete: bool,
    auto_ping: bool,
}

#[derive(Debug, Clone, Copy)]
enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Dialog {
    None,
    AddServer,
    EditServer(i64),
    DeleteServer,
    Search,
    Groups,
    Notes(i64),
    Tags(i64),
    Copy,
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormField {
    Name,
    Host,
    Username,
    Port,
    Group,
    AuthType,
    KeyPath,
    Password,
    Favorite,
    Notes,
    Tags,
}

impl FormField {
    const ALL: [FormField; 11] = [
        FormField::Name,
        FormField::Host,
        FormField::Username,
        FormField::Port,
        FormField::Group,
        FormField::AuthType,
        FormField::KeyPath,
        FormField::Password,
        FormField::Favorite,
        FormField::Notes,
        FormField::Tags,
    ];

    fn label(self) -> &'static str {
        match self {
            FormField::Name => "Name",
            FormField::Host => "Host / Address",
            FormField::Username => "Username",
            FormField::Port => "Port",
            FormField::Group => "Group",
            FormField::AuthType => "Auth",
            FormField::KeyPath => "Key Path",
            FormField::Password => "Password",
            FormField::Favorite => "Favorite",
            FormField::Notes => "Notes",
            FormField::Tags => "Tags",
        }
    }
}

#[derive(Debug, Clone)]
struct ServerForm {
    name: String,
    host: String,
    username: String,
    port: String,
    group_name: String,
    auth_type: usize,
    password: String,
    key_path: String,
    key_passphrase: String,
    notes: String,
    tags: String,
    favorite: bool,
    field: usize,
    cursor: usize,
    error: Option<String>,
    warning: Option<String>,
}

impl ServerForm {
    fn blank(default_port: i64) -> Self {
        Self {
            name: String::new(),
            host: String::new(),
            username: env::var("USER").unwrap_or_else(|_| "user".to_string()),
            port: default_port.to_string(),
            group_name: String::new(),
            auth_type: 0,
            password: String::new(),
            key_path: String::new(),
            key_passphrase: String::new(),
            notes: String::new(),
            tags: String::new(),
            favorite: false,
            field: 0,
            cursor: 0,
            error: None,
            warning: None,
        }
    }

    fn from_server(server: &Server) -> Self {
        Self {
            name: server.name.clone(),
            host: server.host.clone(),
            username: server.username.clone(),
            port: server.port.to_string(),
            group_name: server.group_name.clone(),
            auth_type: match server.auth_type.as_str() {
                "Password" => 1,
                "SSH Agent" => 2,
                _ => 0,
            },
            password: server.password.clone().unwrap_or_default(),
            key_path: server.key_path.clone().unwrap_or_default(),
            key_passphrase: server.key_passphrase.clone().unwrap_or_default(),
            notes: server.notes.clone(),
            tags: server.tags.clone(),
            favorite: server.favorite,
            field: 0,
            cursor: server.name.chars().count(),
            error: None,
            warning: None,
        }
    }

    fn active_field(&self) -> FormField {
        FormField::ALL[self.field]
    }

    fn auth_type(&self) -> &'static str {
        match self.auth_type {
            1 => "Password",
            2 => "SSH Agent",
            _ => "SSH Key",
        }
    }

    fn active_text_value_mut(&mut self) -> Option<&mut String> {
        match self.active_field() {
            FormField::Name => Some(&mut self.name),
            FormField::Host => Some(&mut self.host),
            FormField::Username => Some(&mut self.username),
            FormField::Port => Some(&mut self.port),
            FormField::KeyPath => Some(&mut self.key_path),
            FormField::Password => Some(&mut self.password),
            FormField::Notes => Some(&mut self.notes),
            FormField::Tags => Some(&mut self.tags),
            FormField::Group | FormField::AuthType | FormField::Favorite => None,
        }
    }

    fn active_text_len(&self) -> usize {
        self.current_value_for_render(self.active_field())
            .chars()
            .count()
    }

    fn next_field(&mut self) {
        self.field = (self.field + 1) % FormField::ALL.len();
        self.cursor = self.active_text_len();
    }

    fn previous_field(&mut self) {
        self.field = self
            .field
            .checked_sub(1)
            .unwrap_or(FormField::ALL.len() - 1);
        self.cursor = self.active_text_len();
    }

    fn move_cursor_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    fn move_cursor_right(&mut self) {
        self.cursor = min(self.cursor + 1, self.active_text_len());
    }

    fn insert_char(&mut self, ch: char) {
        let cursor = self.cursor;
        let Some(value) = self.active_text_value_mut() else {
            return;
        };
        self.cursor = insert_char_at(value, cursor, ch);
    }

    fn backspace(&mut self) {
        let cursor = self.cursor;
        let Some(value) = self.active_text_value_mut() else {
            return;
        };
        self.cursor = backspace_at(value, cursor);
    }

    fn delete_char(&mut self) {
        let cursor = self.cursor;
        let Some(value) = self.active_text_value_mut() else {
            return;
        };
        self.cursor = delete_char_at(value, cursor);
    }
}

#[derive(Debug, Clone)]
struct FormRow<'a> {
    line: Line<'a>,
    field: Option<FormField>,
    value_width: u16,
}

impl FormRow<'static> {
    fn section(title: &str) -> Self {
        Self {
            line: Line::from(title.to_string()).fg(theme::CYAN),
            field: None,
            value_width: 0,
        }
    }

    fn message(message: impl Into<String>, color: Color) -> Self {
        Self {
            line: Line::from(message.into()).fg(color),
            field: None,
            value_width: 0,
        }
    }

    fn blank() -> Self {
        Self {
            line: Line::from(""),
            field: None,
            value_width: 0,
        }
    }
}

#[derive(Debug, Clone)]
struct TextEditor {
    value: String,
    error: Option<String>,
}

#[derive(Debug)]
pub struct App {
    db_path: PathBuf,
    conn: Connection,
    servers: Vec<Server>,
    groups: Vec<Group>,
    settings: Settings,
    selected_server: usize,
    selected_group: usize,
    dialog: Dialog,
    form: ServerForm,
    text_editor: TextEditor,
    search_query: String,
    group_input: String,
    group_input_active: bool,
    group_cursor: usize,
    group_editing_id: Option<i64>,
    status: Option<(LogLevel, String)>,
    pending_ssh: Option<SshLaunch>,
    copy_cursor: usize,
    exit: bool,
}

impl App {
    fn new() -> Result<Self> {
        let db_path = default_db_path()?;
        Self::with_db_path(db_path)
    }

    fn with_db_path(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&db_path)?;
        initialize_database(&conn)?;
        let settings = load_settings(&conn)?;
        let mut app = Self {
            db_path,
            conn,
            servers: Vec::new(),
            groups: Vec::new(),
            form: ServerForm::blank(settings.default_port),
            text_editor: TextEditor {
                value: String::new(),
                error: None,
            },
            settings,
            selected_server: 0,
            selected_group: 0,
            dialog: Dialog::None,
            search_query: String::new(),
            group_input: String::new(),
            group_input_active: false,
            group_cursor: 0,
            group_editing_id: None,
            status: None,
            pending_ssh: None,
            copy_cursor: 0,
            exit: false,
        };
        app.reload()?;
        app.status = Some((
            LogLevel::Info,
            if app.servers.is_empty() {
                "Welcome to russhx. Add a server to get started.".to_string()
            } else {
                "Welcome to russhx.".to_string()
            },
        ));
        if let Some(id) = app.settings.last_selected_server {
            if let Some(index) = app
                .filtered_servers()
                .iter()
                .position(|server| server.id == id)
            {
                app.selected_server = index;
            }
        }
        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let area = frame.area();
        let outer = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(theme::BORDER));
        frame.render_widget(outer, area);
        let app_area = area.inner(Margin {
            horizontal: 1,
            vertical: 1,
        });

        let rows = Layout::vertical([
            Constraint::Length(9),
            Constraint::Min(18),
            Constraint::Length(4),
        ])
        .split(app_area);

        self.render_header(frame, rows[0]);
        self.render_body(frame, rows[1]);
        self.render_footer(frame, rows[2]);

        match self.dialog {
            Dialog::None => {}
            Dialog::AddServer | Dialog::EditServer(_) => self.render_server_form(frame, area),
            Dialog::DeleteServer => self.render_delete_dialog(frame, area),
            Dialog::Search => self.render_search_dialog(frame, area),
            Dialog::Groups => self.render_groups_dialog(frame, area),
            Dialog::Notes(_) => self.render_text_dialog(frame, area, "NOTES EDITOR"),
            Dialog::Tags(_) => self.render_text_dialog(frame, area, "TAGS EDITOR"),
            Dialog::Copy => self.render_copy_dialog(frame, area),
            Dialog::Help => self.render_help(frame, area),
        }
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal([Constraint::Percentage(66), Constraint::Percentage(34)])
            .split(area);

        let logo = Text::from(vec![
            Line::from("  ____                _          ").fg(theme::CYAN),
            Line::from(" |  _ \\ _   _ ___ ___| |__ __  __").fg(theme::CYAN),
            Line::from(" | |_) | | | / __/ __| '_ \\\\ \\/ /").fg(theme::CYAN),
            Line::from(" |  _ <| |_| \\__ \\__ \\ | | |>  < ").fg(theme::CYAN),
            Line::from(" |_| \\_\\\\__,_|___/___/_| |_/_/\\_\\").fg(theme::CYAN),
            Line::from("                                  ").fg(theme::CYAN),
            Line::from("  terminal server vault and ssh launcher").fg(theme::TEXT),
        ]);
        frame.render_widget(Paragraph::new(logo), chunks[0]);

        let total = self.servers.len();
        let last = self
            .servers
            .iter()
            .filter(|server| server.last_used.is_some())
            .max_by_key(|server| server.last_used.clone())
            .map(|server| server.name.as_str())
            .unwrap_or("--");
        let overview = Text::from(vec![
            Line::from(vec![
                "  Total Servers: ".into(),
                total.to_string().fg(theme::CYAN),
            ]),
            Line::from(vec![
                "  Groups:        ".into(),
                self.groups.len().to_string().fg(theme::CYAN),
            ]),
            Line::from(vec!["  Last Used:     ".into(), last.fg(theme::PURPLE)]),
        ]);
        frame.render_widget(
            Paragraph::new(overview).block(panel("OVERVIEW")),
            inset(chunks[1], 1, 0),
        );
    }

    fn render_body(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal([
            Constraint::Length(30),
            Constraint::Min(54),
            Constraint::Length(38),
        ])
        .split(area);

        self.render_groups(frame, chunks[0]);
        self.render_servers(frame, chunks[1]);
        self.render_details(frame, chunks[2]);
    }

    fn render_groups(&self, frame: &mut Frame, area: Rect) {
        let filtered_group = self.current_group_filter();
        let mut items = vec![ListItem::new(Line::from(vec![
            "  All Servers ".fg(theme::TEXT),
            self.servers.len().to_string().fg(theme::CYAN),
        ]))];
        for group in &self.groups {
            let count = self
                .servers
                .iter()
                .filter(|server| server.group_name == group.name)
                .count();
            let style = color_from_name(&group.color);
            let icon = if group.icon.trim().is_empty() {
                "+"
            } else {
                &group.icon
            };
            items.push(ListItem::new(Line::from(vec![
                format!("  {icon} ").fg(style),
                group.name.clone().into(),
                format!(" {count}").fg(theme::CYAN),
            ])));
        }

        let mut state = ListState::default();
        state.select(Some(self.selected_group));
        let list = List::new(items)
            .block(panel("GROUPS"))
            .highlight_style(Style::default().fg(Color::White).bg(theme::PURPLE))
            .highlight_symbol(" ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(list, area, &mut state);

        if self.groups.is_empty() {
            let empty = Paragraph::new(Text::from(vec![
                Line::from(""),
                Line::from("No groups yet").centered().fg(theme::MUTED),
                Line::from(""),
                Line::from("Create groups with g")
                    .centered()
                    .fg(theme::MUTED),
            ]));
            let body = area.inner(Margin {
                horizontal: 2,
                vertical: 7,
            });
            frame.render_widget(empty, body);
        }

        let tip = Paragraph::new(Text::from(vec![
            Line::from("Use number keys 1-9"),
            Line::from("to quickly connect"),
            Line::from("to servers"),
            Line::from(filtered_group.unwrap_or("All Servers").fg(theme::CYAN)),
        ]))
        .block(panel("TIP").border_style(Style::default().fg(theme::YELLOW)))
        .wrap(Wrap { trim: true });
        let tip_area = Rect {
            x: area.x + 1,
            y: area.y + area.height.saturating_sub(7),
            width: area.width.saturating_sub(2),
            height: 6,
        };
        if area.height > 13 {
            frame.render_widget(tip, tip_area);
        }
    }

    fn render_servers(&self, frame: &mut Frame, area: Rect) {
        let filtered = self.filtered_servers();
        if filtered.is_empty() {
            frame.render_widget(
                Block::bordered()
                    .title(" SERVERS ")
                    .border_style(theme::BORDER),
                area,
            );
            let content = Text::from(vec![
                Line::from(""),
                Line::from("  .----------------.   .------------.").fg(theme::TEXT),
                Line::from("  | >_             |   |  o |  ---  |").fg(theme::CYAN),
                Line::from("  '----------------'   '------------'").fg(theme::TEXT),
                Line::from(""),
                Line::from("No servers added yet")
                    .centered()
                    .fg(theme::TEXT)
                    .bold(),
                Line::from("Add your first server to get started.")
                    .centered()
                    .fg(theme::MUTED),
                Line::from(""),
                Line::from(vec![
                    "Press ".into(),
                    "a".fg(theme::PURPLE).bold(),
                    " to add a new server".into(),
                ])
                .centered(),
            ]);
            frame.render_widget(
                Paragraph::new(content).alignment(Alignment::Center),
                area.inner(Margin {
                    horizontal: 2,
                    vertical: 4,
                }),
            );
            return;
        }

        let rows = filtered.iter().enumerate().map(|(index, server)| {
            let number = if index < 9 {
                (index + 1).to_string()
            } else if index == 9 {
                "0".to_string()
            } else {
                "-".to_string()
            };
            Row::new(vec![
                Cell::from(number),
                Cell::from(server.name.clone()),
                Cell::from(server.host.clone()),
                Cell::from(server.username.clone()),
                Cell::from(server.group_name.clone()),
                Cell::from(relative_time(server.last_used.as_deref())),
            ])
        });

        let header = Row::new(vec!["#", "Name", "Host", "User", "Group", "Last Used"])
            .style(
                Style::default()
                    .fg(theme::CYAN)
                    .add_modifier(Modifier::BOLD),
            )
            .bottom_margin(1);
        let table = Table::new(
            rows,
            [
                Constraint::Length(3),
                Constraint::Percentage(26),
                Constraint::Percentage(24),
                Constraint::Percentage(18),
                Constraint::Percentage(18),
                Constraint::Length(12),
            ],
        )
        .header(header)
        .block(panel("SERVERS"))
        .row_highlight_style(Style::default().fg(Color::White).bg(theme::PURPLE))
        .highlight_symbol(" ")
        .highlight_spacing(HighlightSpacing::Always)
        .column_spacing(1);
        let mut state = TableState::default();
        state.select(Some(min(
            self.selected_server,
            filtered.len().saturating_sub(1),
        )));
        frame.render_stateful_widget(table, area, &mut state);
    }

    fn render_details(&self, frame: &mut Frame, area: Rect) {
        let block = panel("SERVER DETAILS");
        if let Some(server) = self.selected_server() {
            let port = server.port.to_string();
            let auth_line = if server.auth_type == "Password" {
                "Password"
            } else if server.auth_type == "SSH Agent" {
                "SSH Agent"
            } else {
                "SSH Key"
            };
            let mut lines = vec![
                Line::from(server.name.clone()).fg(theme::PURPLE).bold(),
                Line::from(""),
                detail_line("Host", &server.host),
                detail_line("User", &server.username),
                detail_line("Port", &port),
                detail_line("Group", or_dash(&server.group_name)),
                detail_line("Auth", auth_line),
                detail_line("Key", server.key_path.as_deref().unwrap_or("--")),
                detail_line("Created", &server.created_at),
                detail_line("Updated", &server.updated_at),
                detail_line("Last Used", server.last_used.as_deref().unwrap_or("--")),
                Line::from(""),
                Line::from("NOTES").fg(theme::CYAN),
                Line::from(if server.notes.trim().is_empty() {
                    "No notes yet.".fg(theme::MUTED)
                } else {
                    server.notes.clone().into()
                }),
                Line::from(""),
                Line::from("TAGS").fg(theme::CYAN),
            ];
            lines.extend(tag_chip_lines(&server.tags, 24));
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .block(block)
                    .wrap(Wrap { trim: true }),
                area,
            );
        } else {
            let text = Text::from(vec![
                Line::from(""),
                Line::from(".--------.").centered().fg(theme::TEXT),
                Line::from("|        |").centered().fg(theme::TEXT),
                Line::from("'--------'").centered().fg(theme::TEXT),
                Line::from(""),
                Line::from("No server selected")
                    .centered()
                    .fg(theme::PURPLE),
                Line::from("Select a server from the list")
                    .centered()
                    .fg(theme::MUTED),
                Line::from("to view its details.")
                    .centered()
                    .fg(theme::MUTED),
                Line::from(""),
                Line::from("NOTES").fg(theme::CYAN),
                Line::from("You can add notes after creating a server.").fg(theme::MUTED),
                Line::from(""),
                Line::from("TAGS").fg(theme::CYAN),
                Line::from("No tags available yet.").fg(theme::MUTED),
            ]);
            frame.render_widget(
                Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
                area,
            );
        }
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let keys = [
            ("ENTER", "Connect"),
            ("a", "Add"),
            ("e", "Edit"),
            ("d", "Delete"),
            ("g", "Groups"),
            ("c", "Copy"),
            ("n", "Notes"),
            ("t", "Tags"),
            ("/", "Search"),
            ("←/→", "Groups"),
            ("?", "Help"),
            ("q", "Quit"),
        ];
        let mut spans = Vec::new();
        for (key, label) in keys {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(theme::PURPLE)
                    .add_modifier(Modifier::BOLD)
                    .bg(theme::PANEL),
            ));
            spans.push(Span::raw(format!(" {label}   ")));
        }
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(2)]).split(area);
        let status = self
            .status
            .as_ref()
            .map(|(level, message)| {
                let color = match level {
                    LogLevel::Info => theme::CYAN,
                    LogLevel::Success => theme::GREEN,
                    LogLevel::Warning => theme::YELLOW,
                    LogLevel::Error => theme::RED,
                };
                Line::from(vec![" ".into(), message.clone().fg(color)])
            })
            .unwrap_or_else(|| Line::from(""));
        frame.render_widget(Paragraph::new(status), rows[0]);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::TOP)),
            rows[1],
        );
    }

    fn render_server_form(&self, frame: &mut Frame, area: Rect) {
        let title = if matches!(self.dialog, Dialog::AddServer) {
            "ADD NEW SERVER"
        } else {
            "EDIT SERVER"
        };
        let popup = centered_rect(area, 80, 88);
        frame.render_widget(Clear, popup);
        frame.render_widget(panel(title), popup);
        let inner = popup.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let vertical = Layout::vertical([Constraint::Min(10), Constraint::Length(3)]).split(inner);
        let columns = Layout::horizontal([Constraint::Percentage(68), Constraint::Percentage(32)])
            .split(vertical[0]);

        frame.render_widget(panel("SERVER"), columns[0]);
        let form_body = columns[0].inner(Margin {
            horizontal: 1,
            vertical: 1,
        });
        let form_rows = self.form_rows(form_body.width.saturating_sub(1));
        let active_field = self.form.active_field();
        let active_row = form_rows
            .iter()
            .position(|row| row.field == Some(active_field))
            .unwrap_or(0);
        let visible_height = form_body.height as usize;
        let scroll = if active_row >= visible_height {
            active_row + 1 - visible_height
        } else {
            0
        };
        let visible_rows = form_rows
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|row| row.line.clone())
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(visible_rows), form_body);

        frame.render_widget(
            Paragraph::new(self.preview_lines())
                .block(panel("PREVIEW"))
                .wrap(Wrap { trim: true }),
            columns[1],
        );
        self.render_dialog_footer(
            frame,
            vertical[1],
            &[
                ("TAB", "Next field"),
                ("Shift+TAB", "Previous"),
                ("ENTER", "Save"),
                ("ESC", "Cancel"),
            ],
        );
        if let Some(row) = form_rows.get(active_row) {
            if row.field != Some(FormField::AuthType) && row.field != Some(FormField::Favorite) {
                let x = form_body.x + FORM_INPUT_COL + row.value_width;
                let y = form_body.y + active_row.saturating_sub(scroll) as u16;
                set_clamped_cursor(frame, form_body, x, y);
            }
        }
    }

    fn render_delete_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 42, 28);
        frame.render_widget(Clear, popup);
        let name = self
            .selected_server()
            .map(|server| server.name.as_str())
            .unwrap_or("selected server");
        let text = Text::from(vec![
            Line::from("Delete Server?")
                .centered()
                .fg(theme::RED)
                .bold(),
            Line::from(""),
            Line::from(name).centered().fg(theme::PURPLE),
            Line::from(""),
            Line::from("This action cannot be undone.")
                .centered()
                .fg(theme::MUTED),
            Line::from(""),
            Line::from(vec![
                "ENTER".fg(theme::RED).bold(),
                " Delete    ".into(),
                "ESC".fg(theme::PURPLE).bold(),
                " Cancel".into(),
            ])
            .centered(),
        ]);
        frame.render_widget(Paragraph::new(text).block(panel("CONFIRM DELETE")), popup);
    }

    fn render_search_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 50, 28);
        frame.render_widget(Clear, popup);
        frame.render_widget(panel("SEARCH"), popup);
        let inner = popup.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let vertical = Layout::vertical([Constraint::Min(4), Constraint::Length(3)]).split(inner);
        let input_width = vertical[0].width.saturating_sub(4);
        let visible_query = visible_tail(&self.search_query, input_width);
        let count = self.filtered_servers().len();
        let text = Text::from(vec![
            Line::from("Search").fg(theme::CYAN).bold(),
            Line::from(""),
            Line::from(vec![
                "> ".fg(theme::PURPLE),
                visible_query.clone().fg(theme::TEXT),
            ]),
            Line::from(""),
            Line::from("Searches name, host, username, tags, and group.").fg(theme::MUTED),
            Line::from(format!("{count} result(s)")).fg(theme::PURPLE),
        ]);
        frame.render_widget(Paragraph::new(text), vertical[0]);
        self.render_dialog_footer(frame, vertical[1], &[("ENTER", "Apply"), ("ESC", "Clear")]);
        set_clamped_cursor(
            frame,
            vertical[0],
            vertical[0].x + 2 + visible_query.len() as u16,
            vertical[0].y + 2,
        );
    }

    fn render_groups_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 50, 54);
        frame.render_widget(Clear, popup);
        frame.render_widget(panel("GROUP MANAGER"), popup);
        let inner = popup.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let vertical = Layout::vertical([Constraint::Min(7), Constraint::Length(3)]).split(inner);
        let mut lines = vec![Line::from("Groups").fg(theme::CYAN).bold(), Line::from("")];
        let fixed_rows = 7usize;
        let max_groups = vertical[0].height.saturating_sub(fixed_rows as u16).max(1) as usize;
        if self.groups.is_empty() {
            lines.push(Line::from("No groups yet.").fg(theme::MUTED));
        } else {
            let start = if self.group_cursor >= max_groups {
                self.group_cursor + 1 - max_groups
            } else {
                0
            };
            for (index, group) in self.groups.iter().enumerate().skip(start).take(max_groups) {
                let name_style = if index == self.group_cursor {
                    Style::default().fg(Color::White).bg(theme::PURPLE)
                } else {
                    Style::default().fg(theme::TEXT)
                };
                lines.push(Line::from(vec![
                    format!("{} ", group.icon).fg(color_from_name(&group.color)),
                    Span::styled(group.name.clone(), name_style),
                    format!("  #{}", group.id).fg(theme::MUTED),
                ]));
            }
            if self.groups.len() > start + max_groups {
                lines.push(
                    Line::from(format!(
                        "... {} more",
                        self.groups.len() - start - max_groups
                    ))
                    .fg(theme::MUTED),
                );
            }
        }
        let input_width = vertical[0].width.saturating_sub(4);
        let visible_input = visible_tail(&self.group_input, input_width);
        lines.extend([
            Line::from(""),
            Line::from("----------------"),
            Line::from(if self.group_editing_id.is_some() {
                "Edit group name"
            } else if self.group_input_active {
                "New group name"
            } else {
                "Press n for a new group"
            })
            .fg(theme::CYAN),
            Line::from(vec![
                "> ".fg(theme::PURPLE),
                visible_input.clone().fg(theme::TEXT),
            ]),
        ]);
        let input_y = vertical[0].y + lines.len().saturating_sub(1) as u16;
        frame.render_widget(Paragraph::new(lines), vertical[0]);
        self.render_dialog_footer(
            frame,
            vertical[1],
            &[
                ("↑/↓", "Select"),
                ("n", "New"),
                ("e", "Edit"),
                ("d", "Delete"),
                ("ENTER", "Save"),
                ("ESC", "Close"),
            ],
        );
        if self.group_input_active {
            set_clamped_cursor(
                frame,
                vertical[0],
                vertical[0].x + 2 + visible_input.len() as u16,
                input_y,
            );
        }
    }

    fn render_text_dialog(&self, frame: &mut Frame, area: Rect, title: &str) {
        let popup = centered_rect(area, 58, 42);
        frame.render_widget(Clear, popup);
        frame.render_widget(panel(title), popup);
        let inner = popup.inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        let vertical = Layout::vertical([Constraint::Min(6), Constraint::Length(3)]).split(inner);
        let editor = vertical[0].inner(Margin {
            horizontal: 0,
            vertical: 2,
        });
        let visible = visible_editor_lines(&self.text_editor.value, editor.width, editor.height);
        let mut lines = vec![Line::from("Edit value").fg(theme::CYAN), Line::from("")];
        lines.extend(
            visible
                .iter()
                .map(|line| Line::from(line.clone()).fg(theme::TEXT)),
        );
        if let Some(error) = &self.text_editor.error {
            lines.push(Line::from(""));
            lines.push(Line::from(error.clone()).fg(theme::RED));
        }
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }),
            vertical[0],
        );
        self.render_dialog_footer(frame, vertical[1], &[("ENTER", "Save"), ("ESC", "Cancel")]);
        let (cursor_x, cursor_y) = editor_cursor(&self.text_editor.value, editor, visible.len());
        set_clamped_cursor(frame, editor, cursor_x, cursor_y);
    }

    fn render_copy_dialog(&self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 42, 36);
        frame.render_widget(Clear, popup);
        let options = ["IP", "Username", "SSH Command", "Host", "SSH Key Path"];
        let items = options
            .iter()
            .map(|option| ListItem::new(*option))
            .collect::<Vec<_>>();
        let mut state = ListState::default();
        state.select(Some(self.copy_cursor));
        let list = List::new(items)
            .block(panel("COPY"))
            .highlight_style(Style::default().bg(theme::PURPLE))
            .highlight_symbol(" ")
            .highlight_spacing(HighlightSpacing::Always);
        frame.render_stateful_widget(list, popup, &mut state);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
        let popup = centered_rect(area, 78, 86);
        frame.render_widget(Clear, popup);
        let text = Text::from(vec![
            Line::from("russhx help").fg(theme::CYAN).bold(),
            Line::from(""),
            shortcut_line("Enter", "Connect to selected server"),
            shortcut_line("a", "Add server"),
            shortcut_line("e", "Edit server"),
            shortcut_line("d/Delete", "Delete server"),
            shortcut_line("g", "Manage groups"),
            shortcut_line("←/→", "Switch group filter"),
            shortcut_line("/", "Search by name, host, username, tags, group"),
            shortcut_line("c", "Copy server values"),
            shortcut_line("t", "Edit tags"),
            shortcut_line("n", "Edit notes"),
            shortcut_line("q", "Quit"),
            Line::from(""),
            Line::from(format!("Database: {}", self.db_path.display())).fg(theme::MUTED),
            Line::from(format!("Theme: {}", self.settings.theme)).fg(theme::MUTED),
            Line::from(format!("Auto ping: {}", self.settings.auto_ping)).fg(theme::MUTED),
            Line::from(format!(
                "Confirm before delete: {}",
                self.settings.confirm_before_delete
            ))
            .fg(theme::MUTED),
            Line::from(format!("Version: {}", env!("CARGO_PKG_VERSION"))).fg(theme::MUTED),
            Line::from(""),
            Line::from(vec!["ESC".fg(theme::PURPLE).bold(), " Close help".into()]),
        ]);
        frame.render_widget(
            Paragraph::new(text)
                .block(panel("HELP"))
                .wrap(Wrap { trim: true }),
            popup,
        );
    }

    fn handle_events(&mut self) -> Result<()> {
        match event::read()? {
            Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                self.handle_key_events(key_event)?
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key_events(&mut self, key_event: KeyEvent) -> Result<()> {
        if key_event.code == KeyCode::Char('c')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.exit = true;
            return Ok(());
        }

        if self.dialog != Dialog::None {
            return self.handle_dialog_key(key_event);
        }

        match key_event.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char('a') => self.open_add_dialog(),
            KeyCode::Char('e') => self.open_edit_dialog(),
            KeyCode::Char('d') | KeyCode::Delete => {
                if self.selected_server().is_some() {
                    if self.settings.confirm_before_delete {
                        self.dialog = Dialog::DeleteServer;
                    } else {
                        self.delete_selected_server()?;
                    }
                }
            }
            KeyCode::Char('g') => {
                self.group_input.clear();
                self.group_input_active = false;
                self.group_editing_id = None;
                self.group_cursor = min(self.group_cursor, self.groups.len().saturating_sub(1));
                self.dialog = Dialog::Groups;
            }
            KeyCode::Char('/') | KeyCode::Char('f') => self.dialog = Dialog::Search,
            KeyCode::Char('?') => self.dialog = Dialog::Help,
            KeyCode::Char('c') => {
                if self.selected_server().is_some() {
                    self.copy_cursor = 0;
                    self.dialog = Dialog::Copy;
                }
            }
            KeyCode::Char('n') => self.open_text_editor(true),
            KeyCode::Char('t') => self.open_text_editor(false),
            KeyCode::Up => self.select_previous_server(),
            KeyCode::Down => self.select_next_server(),
            KeyCode::Left => self.select_previous_group(),
            KeyCode::Right => self.select_next_group(),
            KeyCode::Enter => self.connect_selected()?,
            KeyCode::Char(ch) if ch.is_ascii_digit() => self.connect_by_number(ch)?,
            _ => {}
        }
        Ok(())
    }

    fn handle_dialog_key(&mut self, key_event: KeyEvent) -> Result<()> {
        if key_event.code == KeyCode::Char('c')
            && key_event.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.exit = true;
            self.dialog = Dialog::None;
            return Ok(());
        }

        match self.dialog.clone() {
            Dialog::AddServer | Dialog::EditServer(_) => self.handle_form_key(key_event),
            Dialog::DeleteServer => match key_event.code {
                KeyCode::Esc => self.dialog = Dialog::None,
                KeyCode::Enter | KeyCode::Char('d') => self.delete_selected_server()?,
                _ => {}
            },
            Dialog::Search => match key_event.code {
                KeyCode::Esc => {
                    self.search_query.clear();
                    self.selected_server = 0;
                    self.dialog = Dialog::None;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.selected_server = 0;
                }
                KeyCode::Enter => self.dialog = Dialog::None,
                KeyCode::Char(ch) => {
                    self.search_query.push(ch);
                    self.selected_server = 0;
                }
                _ => {}
            },
            Dialog::Groups => self.handle_groups_key(key_event)?,
            Dialog::Notes(id) => self.handle_text_key(key_event, id, true)?,
            Dialog::Tags(id) => self.handle_text_key(key_event, id, false)?,
            Dialog::Copy => self.handle_copy_key(key_event),
            Dialog::Help => {
                if matches!(
                    key_event.code,
                    KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
                ) {
                    self.dialog = Dialog::None;
                }
            }
            Dialog::None => {}
        }
        Ok(())
    }

    fn handle_form_key(&mut self, key_event: KeyEvent) {
        self.form.error = None;
        self.form.warning = None;
        match key_event.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Tab | KeyCode::Down => {
                self.form.next_field();
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.form.previous_field();
            }
            KeyCode::Left if self.form.active_field() == FormField::AuthType => {
                self.form.auth_type = self.form.auth_type.checked_sub(1).unwrap_or(2);
            }
            KeyCode::Right if self.form.active_field() == FormField::AuthType => {
                self.form.auth_type = (self.form.auth_type + 1) % 3;
            }
            KeyCode::Left if self.form.active_field() == FormField::Group => {
                self.select_form_group(-1);
            }
            KeyCode::Right if self.form.active_field() == FormField::Group => {
                self.select_form_group(1);
            }
            KeyCode::Left if self.form.active_text_value_mut().is_some() => {
                self.form.move_cursor_left();
            }
            KeyCode::Right if self.form.active_text_value_mut().is_some() => {
                self.form.move_cursor_right();
            }
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                if self.form.active_field() == FormField::Favorite =>
            {
                self.form.favorite = !self.form.favorite;
            }
            KeyCode::Enter => {
                if let Err(error) = self.save_form() {
                    let message = error.to_string();
                    self.form.error = Some(message.clone());
                    self.push_log(LogLevel::Error, message);
                }
            }
            KeyCode::Backspace => {
                self.form.backspace();
            }
            KeyCode::Delete => {
                self.form.delete_char();
            }
            KeyCode::Char('f') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                self.form.favorite = !self.form.favorite;
            }
            KeyCode::Char(ch) => {
                if self.form.active_field() == FormField::AuthType {
                    match ch {
                        'k' => self.form.auth_type = 0,
                        'p' => self.form.auth_type = 1,
                        'a' => self.form.auth_type = 2,
                        _ => {}
                    }
                } else if self.form.active_field() == FormField::Group {
                    self.select_form_group_by_prefix(ch);
                } else if self.form.active_field() == FormField::Favorite {
                    if matches!(ch, 'y' | 'Y' | '1') {
                        self.form.favorite = true;
                    } else if matches!(ch, 'n' | 'N' | '0') {
                        self.form.favorite = false;
                    }
                } else {
                    self.form.insert_char(ch);
                }
            }
            _ => {}
        }
    }

    fn handle_groups_key(&mut self, key_event: KeyEvent) -> Result<()> {
        match key_event.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Up if !self.group_input_active => {
                if !self.groups.is_empty() {
                    self.group_cursor = self
                        .group_cursor
                        .checked_sub(1)
                        .unwrap_or(self.groups.len() - 1);
                }
            }
            KeyCode::Down if !self.group_input_active => {
                if !self.groups.is_empty() {
                    self.group_cursor = (self.group_cursor + 1) % self.groups.len();
                }
            }
            KeyCode::Char('n') if !self.group_input_active => {
                self.group_input.clear();
                self.group_editing_id = None;
                self.group_input_active = true;
            }
            KeyCode::Char('e') if !self.group_input_active => {
                if let Some(group) = self.groups.get(self.group_cursor) {
                    self.group_input = group.name.clone();
                    self.group_editing_id = Some(group.id);
                    self.group_input_active = true;
                }
            }
            KeyCode::Char('d') | KeyCode::Delete if !self.group_input_active => {
                self.delete_selected_group()?;
            }
            KeyCode::Backspace => {
                if self.group_input_active {
                    self.group_input.pop();
                }
            }
            KeyCode::Enter => {
                if !self.group_input_active {
                    return Ok(());
                }
                let name = self.group_input.trim().to_string();
                if !name.is_empty() {
                    if let Some(id) = self.group_editing_id {
                        if let Err(error) = self.rename_group(id, &name) {
                            self.push_log(LogLevel::Error, error.to_string());
                            return Ok(());
                        }
                    } else {
                        self.conn.execute(
                            "INSERT OR IGNORE INTO groups (name, color, icon) VALUES (?1, ?2, ?3)",
                            params![name, "green", "+"],
                        )?;
                    }
                    self.group_input.clear();
                    self.group_editing_id = None;
                    self.group_input_active = false;
                    self.reload()?;
                    self.group_cursor = min(self.group_cursor, self.groups.len().saturating_sub(1));
                    self.push_log(LogLevel::Success, "Group saved");
                }
            }
            KeyCode::Char(ch) if self.group_input_active => self.group_input.push(ch),
            _ => {}
        }
        Ok(())
    }

    fn handle_text_key(&mut self, key_event: KeyEvent, server_id: i64, notes: bool) -> Result<()> {
        self.text_editor.error = None;
        match key_event.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Enter if key_event.modifiers.contains(KeyModifiers::SHIFT) => {
                self.text_editor.value.push('\n');
            }
            KeyCode::Enter => {
                let column = if notes { "notes" } else { "tags" };
                let sql = format!(
                    "UPDATE servers SET {column} = ?1, updated_at = datetime('now') WHERE id = ?2"
                );
                self.conn
                    .execute(&sql, params![self.text_editor.value, server_id])?;
                self.reload()?;
                self.dialog = Dialog::None;
                self.push_log(
                    LogLevel::Success,
                    if notes {
                        "Notes updated"
                    } else {
                        "Tags updated"
                    },
                );
            }
            KeyCode::Backspace => {
                self.text_editor.value.pop();
            }
            KeyCode::Char(ch) => self.text_editor.value.push(ch),
            _ => {}
        }
        Ok(())
    }

    fn handle_copy_key(&mut self, key_event: KeyEvent) {
        match key_event.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Up => {
                self.copy_cursor = self.copy_cursor.checked_sub(1).unwrap_or(4);
            }
            KeyCode::Down => {
                self.copy_cursor = (self.copy_cursor + 1) % 5;
            }
            KeyCode::Enter => {
                if let Some(value) = self.copy_value(self.copy_cursor) {
                    if copy_to_clipboard(&value).is_ok() {
                        self.push_log(LogLevel::Success, "Copied value to clipboard");
                    } else {
                        self.push_log(
                            LogLevel::Warning,
                            "Clipboard unavailable; value shown in copy menu",
                        );
                    }
                }
                self.dialog = Dialog::None;
            }
            _ => {}
        }
    }

    fn open_add_dialog(&mut self) {
        self.form = ServerForm::blank(self.settings.default_port);
        self.dialog = Dialog::AddServer;
    }

    fn open_edit_dialog(&mut self) {
        if let Some(server) = self.selected_server().cloned() {
            self.form = ServerForm::from_server(&server);
            self.dialog = Dialog::EditServer(server.id);
        }
    }

    fn open_text_editor(&mut self, notes: bool) {
        if let Some(server) = self.selected_server().cloned() {
            self.text_editor = TextEditor {
                value: if notes {
                    server.notes.clone()
                } else {
                    server.tags.clone()
                },
                error: None,
            };
            self.dialog = if notes {
                Dialog::Notes(server.id)
            } else {
                Dialog::Tags(server.id)
            };
        }
    }

    fn save_form(&mut self) -> Result<()> {
        let edit_id = match self.dialog {
            Dialog::EditServer(id) => Some(id),
            _ => None,
        };
        let name = self.form.name.trim();
        let host = self.form.host.trim();
        let username = self.form.username.trim();
        if name.is_empty() {
            return Err(eyre!("Name is required"));
        }
        if host.is_empty() {
            return Err(eyre!("Host is required"));
        }
        if username.is_empty() {
            return Err(eyre!("Username is required"));
        }
        let port: i64 = self
            .form
            .port
            .trim()
            .parse()
            .map_err(|_| eyre!("Port must be numeric"))?;
        if self
            .servers
            .iter()
            .any(|server| server.name == name && Some(server.id) != edit_id)
        {
            return Err(eyre!("A server with this name already exists"));
        }
        if self.form.auth_type() == "SSH Key"
            && !self.form.key_path.trim().is_empty()
            && !Path::new(self.form.key_path.trim()).exists()
        {
            self.form.warning = Some("Key file does not exist. Saved with warning.".to_string());
        }

        let key_path = empty_to_none(&self.form.key_path);
        let password = empty_to_none(&self.form.password);
        let key_passphrase = empty_to_none(&self.form.key_passphrase);
        let group = self.form.group_name.trim();
        if !group.is_empty() {
            self.conn.execute(
                "INSERT OR IGNORE INTO groups (name, color, icon) VALUES (?1, ?2, ?3)",
                params![group, "green", "+"],
            )?;
        }

        let selected_id = if let Some(id) = edit_id {
            self.conn.execute(
                "UPDATE servers SET
                    name = ?1, host = ?2, username = ?3, port = ?4, group_name = ?5,
                    auth_type = ?6, password = ?7, key_path = ?8, key_passphrase = ?9,
                    notes = ?10, tags = ?11, updated_at = datetime('now'),
                    favorite = ?12
                 WHERE id = ?13",
                params![
                    name,
                    host,
                    username,
                    port,
                    group,
                    self.form.auth_type(),
                    password,
                    key_path,
                    key_passphrase,
                    self.form.notes.trim(),
                    self.form.tags.trim(),
                    self.form.favorite as i64,
                    id
                ],
            )?;
            self.push_log(LogLevel::Success, format!("Updated server {name}"));
            id
        } else {
            self.conn.execute(
                "INSERT INTO servers (
                    name, host, username, port, group_name, auth_type, password, key_path,
                    key_passphrase, notes, tags, created_at, updated_at, last_used, favorite
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                    datetime('now'), datetime('now'), NULL, ?12
                 )",
                params![
                    name,
                    host,
                    username,
                    port,
                    group,
                    self.form.auth_type(),
                    password,
                    key_path,
                    key_passphrase,
                    self.form.notes.trim(),
                    self.form.tags.trim(),
                    self.form.favorite as i64
                ],
            )?;
            let id = self.conn.last_insert_rowid();
            self.push_log(LogLevel::Success, format!("Added server {name}"));
            id
        };
        self.reload()?;
        self.focus_server(selected_id);
        self.dialog = Dialog::None;
        Ok(())
    }

    fn delete_selected_server(&mut self) -> Result<()> {
        let Some(server) = self.selected_server().cloned() else {
            self.dialog = Dialog::None;
            return Ok(());
        };
        self.conn
            .execute("DELETE FROM servers WHERE id = ?1", params![server.id])?;
        self.reload()?;
        let len = self.filtered_servers().len();
        if len == 0 {
            self.selected_server = 0;
        } else if self.selected_server >= len {
            self.selected_server = len - 1;
        }
        self.dialog = Dialog::None;
        self.push_log(LogLevel::Warning, format!("Deleted server {}", server.name));
        Ok(())
    }

    fn connect_selected(&mut self) -> Result<()> {
        let Some(server) = self.selected_server().cloned() else {
            return Ok(());
        };
        if server.auth_type == "Password" {
            self.push_log(
                LogLevel::Warning,
                "Password authentication is not implemented yet.",
            );
            return Ok(());
        }
        let launch = ssh_launch(&server);
        self.conn.execute(
            "UPDATE servers SET last_used = datetime('now'), updated_at = datetime('now') WHERE id = ?1",
            params![server.id],
        )?;
        self.conn.execute(
            "UPDATE settings SET last_selected_server = ?1 WHERE id = 1",
            params![server.id],
        )?;
        self.reload()?;
        self.focus_server(server.id);
        self.pending_ssh = Some(launch);
        self.exit = true;
        Ok(())
    }

    fn connect_by_number(&mut self, ch: char) -> Result<()> {
        let index = match ch {
            '1'..='9' => ch as usize - '1' as usize,
            '0' => 9,
            _ => return Ok(()),
        };
        if index < self.filtered_servers().len() {
            self.selected_server = index;
            self.connect_selected()?;
        }
        Ok(())
    }

    fn reload(&mut self) -> Result<()> {
        self.groups = load_groups(&self.conn)?;
        self.servers = load_servers(&self.conn)?;
        self.servers
            .sort_by_key(|server| server.name.to_lowercase());
        Ok(())
    }

    fn filtered_servers(&self) -> Vec<&Server> {
        let group_filter = self.current_group_filter();
        let query = self.search_query.to_lowercase();
        self.servers
            .iter()
            .filter(|server| {
                group_filter.is_none_or(|group| server.group_name == group)
                    && (query.is_empty()
                        || server.name.to_lowercase().contains(&query)
                        || server.host.to_lowercase().contains(&query)
                        || server.username.to_lowercase().contains(&query)
                        || server.group_name.to_lowercase().contains(&query)
                        || server.tags.to_lowercase().contains(&query))
            })
            .collect()
    }

    fn selected_server(&self) -> Option<&Server> {
        self.filtered_servers().get(self.selected_server).copied()
    }

    fn focus_server(&mut self, id: i64) {
        if let Some(index) = self
            .filtered_servers()
            .iter()
            .position(|server| server.id == id)
        {
            self.selected_server = index;
        }
    }

    fn current_group_filter(&self) -> Option<&str> {
        if self.selected_group == 0 {
            None
        } else {
            self.groups
                .get(self.selected_group - 1)
                .map(|group| group.name.as_str())
        }
    }

    fn select_previous_server(&mut self) {
        let len = self.filtered_servers().len();
        if len > 0 {
            self.selected_server = self.selected_server.checked_sub(1).unwrap_or(len - 1);
        }
    }

    fn select_next_server(&mut self) {
        let len = self.filtered_servers().len();
        if len > 0 {
            self.selected_server = (self.selected_server + 1) % len;
        }
    }

    fn select_previous_group(&mut self) {
        let len = self.groups.len() + 1;
        self.selected_group = self.selected_group.checked_sub(1).unwrap_or(len - 1);
        self.selected_server = 0;
    }

    fn select_next_group(&mut self) {
        let len = self.groups.len() + 1;
        self.selected_group = (self.selected_group + 1) % len;
        self.selected_server = 0;
    }

    fn copy_value(&self, cursor: usize) -> Option<String> {
        let server = self.selected_server()?;
        Some(match cursor {
            0 => server.host.clone(),
            1 => server.username.clone(),
            2 => ssh_command(server),
            3 => format!("{}@{}", server.username, server.host),
            4 => server.key_path.clone().unwrap_or_default(),
            _ => return None,
        })
    }

    fn select_form_group(&mut self, delta: isize) {
        let mut options = Vec::with_capacity(self.groups.len() + 1);
        options.push(String::new());
        options.extend(self.groups.iter().map(|group| group.name.clone()));
        if options.is_empty() {
            return;
        }
        let current = options
            .iter()
            .position(|name| *name == self.form.group_name)
            .unwrap_or(0);
        let len = options.len() as isize;
        let next = (current as isize + delta).rem_euclid(len) as usize;
        self.form.group_name = options[next].clone();
    }

    fn select_form_group_by_prefix(&mut self, ch: char) {
        let needle = ch.to_ascii_lowercase();
        if let Some(group) = self
            .groups
            .iter()
            .find(|group| group.name.to_ascii_lowercase().starts_with(needle))
        {
            self.form.group_name = group.name.clone();
        }
    }

    fn rename_group(&mut self, id: i64, new_name: &str) -> Result<()> {
        let old_name = self
            .groups
            .iter()
            .find(|group| group.id == id)
            .map(|group| group.name.clone())
            .ok_or_else(|| eyre!("Selected group no longer exists"))?;
        if self
            .groups
            .iter()
            .any(|group| group.id != id && group.name == new_name)
        {
            return Err(eyre!("A group with this name already exists"));
        }
        self.conn.execute(
            "UPDATE groups SET name = ?1 WHERE id = ?2",
            params![new_name, id],
        )?;
        self.conn.execute(
            "UPDATE servers SET group_name = ?1, updated_at = datetime('now') WHERE group_name = ?2",
            params![new_name, old_name],
        )?;
        Ok(())
    }

    fn delete_selected_group(&mut self) -> Result<()> {
        let Some(group) = self.groups.get(self.group_cursor).cloned() else {
            return Ok(());
        };
        self.conn.execute(
            "UPDATE servers SET group_name = '', updated_at = datetime('now') WHERE group_name = ?1",
            params![group.name],
        )?;
        self.conn
            .execute("DELETE FROM groups WHERE id = ?1", params![group.id])?;
        self.reload()?;
        self.group_cursor = min(self.group_cursor, self.groups.len().saturating_sub(1));
        self.group_input.clear();
        self.group_input_active = false;
        self.group_editing_id = None;
        self.push_log(LogLevel::Warning, format!("Deleted group {}", group.name));
        Ok(())
    }

    fn push_log(&mut self, level: LogLevel, message: impl Into<String>) {
        self.status = Some((level, message.into()));
    }

    fn form_rows(&self, width: u16) -> Vec<FormRow<'static>> {
        let active = self.form.active_field();
        let mut rows = vec![
            FormRow::section("BASIC DETAILS"),
            self.form_field_row(FormField::Name, active, width, true),
            self.form_field_row(FormField::Host, active, width, true),
            self.form_field_row(FormField::Username, active, width, true),
            self.form_field_row(FormField::Port, active, width, true),
            self.form_field_row(FormField::Group, active, width, false),
            FormRow::blank(),
            FormRow::section("AUTHENTICATION"),
            self.form_field_row(FormField::AuthType, active, width, false),
            self.form_field_row(FormField::KeyPath, active, width, false),
            self.form_field_row(FormField::Password, active, width, false),
            FormRow::blank(),
            FormRow::section("METADATA"),
            self.form_field_row(FormField::Favorite, active, width, false),
            FormRow::blank(),
            FormRow::section("NOTES"),
            self.form_field_row(FormField::Notes, active, width, false),
            FormRow::blank(),
            FormRow::section("TAGS"),
            self.form_field_row(FormField::Tags, active, width, false),
            FormRow::blank(),
            FormRow::section("BUTTONS"),
            FormRow::message("Use ENTER to save or ESC to cancel.", theme::MUTED),
            FormRow::message("* required field. Use ←/→ to edit text.", theme::MUTED),
        ];
        if let Some(error) = &self.form.error {
            rows.push(FormRow::blank());
            rows.push(FormRow::message(error.clone(), theme::RED));
        }
        if let Some(warning) = &self.form.warning {
            rows.push(FormRow::message(warning.clone(), theme::YELLOW));
        }
        rows
    }

    fn form_field_row(
        &self,
        field: FormField,
        active: FormField,
        width: u16,
        required: bool,
    ) -> FormRow<'static> {
        let is_active = field == active;
        if field == FormField::AuthType {
            let mut spans = vec![
                Span::styled(
                    if is_active { ">" } else { " " },
                    Style::default().fg(if is_active {
                        theme::PURPLE
                    } else {
                        theme::TEXT
                    }),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<width$}", field.label(), width = FORM_LABEL_WIDTH),
                    Style::default().fg(if is_active {
                        theme::PURPLE
                    } else {
                        theme::TEXT
                    }),
                ),
            ];
            for (index, label) in ["SSH Key", "Password", "Agent"].iter().enumerate() {
                let selected = self.form.auth_type == index;
                spans.push(Span::styled(
                    format!(" {label} "),
                    if selected {
                        Style::default()
                            .fg(Color::White)
                            .bg(theme::PURPLE)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme::MUTED)
                    },
                ));
            }
            return FormRow {
                line: Line::from(spans),
                field: Some(field),
                value_width: 0,
            };
        }
        if field == FormField::Group {
            let mut spans = vec![
                Span::styled(
                    if is_active { ">" } else { " " },
                    Style::default().fg(if is_active {
                        theme::PURPLE
                    } else {
                        theme::TEXT
                    }),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("{:<width$}", field.label(), width = FORM_LABEL_WIDTH),
                    Style::default().fg(if is_active {
                        theme::PURPLE
                    } else {
                        theme::TEXT
                    }),
                ),
            ];
            let selected_blank = self.form.group_name.trim().is_empty();
            spans.push(option_span("--", selected_blank));
            if self.groups.is_empty() {
                spans.push(Span::styled(
                    " create with g ",
                    Style::default().fg(theme::MUTED),
                ));
            } else {
                for group in &self.groups {
                    spans.push(option_span(&group.name, self.form.group_name == group.name));
                }
            }
            return FormRow {
                line: Line::from(spans),
                field: Some(field),
                value_width: 0,
            };
        }

        let label_style = if is_active {
            Style::default()
                .fg(theme::PURPLE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let value_style = if is_active {
            Style::default().fg(Color::White)
        } else {
            Style::default().fg(theme::MUTED)
        };
        let available = width.saturating_sub(FORM_INPUT_COL + 1);
        let raw_value = match field {
            FormField::AuthType => self.form.auth_type().to_string(),
            FormField::Favorite => {
                if self.form.favorite {
                    "yes".to_string()
                } else {
                    "no".to_string()
                }
            }
            FormField::Password if !self.form.password.is_empty() => {
                "*".repeat(self.form.password.len())
            }
            _ => self.form.current_value_for_render(field).to_string(),
        };
        let cursor = if is_active {
            self.form.cursor
        } else {
            raw_value.chars().count()
        };
        let (visible_value, cursor_col) = visible_value_and_cursor(&raw_value, cursor, available);
        let label = if required {
            format!("{}*", field.label())
        } else {
            field.label().to_string()
        };
        let spans = vec![
            Span::styled(if is_active { ">" } else { " " }, label_style),
            Span::raw(" "),
            Span::styled(
                format!("{label:<width$}", width = FORM_LABEL_WIDTH),
                label_style,
            ),
            Span::styled(visible_value.clone(), value_style),
        ];
        FormRow {
            line: Line::from(spans),
            field: Some(field),
            value_width: cursor_col,
        }
    }

    fn render_dialog_footer(&self, frame: &mut Frame, area: Rect, shortcuts: &[(&str, &str)]) {
        let mut spans = Vec::new();
        for (key, label) in shortcuts {
            spans.push(Span::styled(
                format!(" {key} "),
                Style::default()
                    .fg(theme::PURPLE)
                    .add_modifier(Modifier::BOLD)
                    .bg(theme::PANEL),
            ));
            spans.push(Span::raw(format!(" {label}   ")));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::TOP)),
            area,
        );
    }

    fn take_pending_ssh(&mut self) -> Option<SshLaunch> {
        self.pending_ssh.take()
    }

    fn preview_lines(&self) -> Text<'_> {
        Text::from(vec![
            Line::from(if self.form.name.trim().is_empty() {
                "New server".fg(theme::PURPLE)
            } else {
                self.form.name.clone().fg(theme::PURPLE)
            })
            .bold(),
            detail_line("Host", or_dash(&self.form.host)),
            detail_line("User", or_dash(&self.form.username)),
            detail_line("Port", or_dash(&self.form.port)),
            detail_line("Group", or_dash(&self.form.group_name)),
            detail_line("Auth", self.form.auth_type()),
            detail_line("Key", or_dash(&self.form.key_path)),
            Line::from(""),
            Line::from("Notes:").fg(theme::CYAN),
            Line::from(if self.form.notes.trim().is_empty() {
                "No notes yet.".fg(theme::MUTED)
            } else {
                self.form.notes.clone().into()
            }),
            Line::from(""),
            Line::from("Tags:").fg(theme::CYAN),
            Line::from(if self.form.tags.trim().is_empty() {
                "No tags yet.".fg(theme::MUTED)
            } else {
                self.form.tags.clone().fg(theme::YELLOW)
            }),
        ])
    }
}

trait FormValue {
    fn current_value_for_render(&self, field: FormField) -> &str;
}

impl FormValue for ServerForm {
    fn current_value_for_render(&self, field: FormField) -> &str {
        match field {
            FormField::Name => &self.name,
            FormField::Host => &self.host,
            FormField::Username => &self.username,
            FormField::Port => &self.port,
            FormField::Group => &self.group_name,
            FormField::AuthType => self.auth_type(),
            FormField::KeyPath => &self.key_path,
            FormField::Password => &self.password,
            FormField::Favorite => {
                if self.favorite {
                    "yes"
                } else {
                    "no"
                }
            }
            FormField::Notes => &self.notes,
            FormField::Tags => &self.tags,
        }
    }
}

fn initialize_database(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS servers (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            host TEXT NOT NULL,
            username TEXT NOT NULL,
            port INTEGER NOT NULL,
            group_name TEXT NOT NULL DEFAULT '',
            auth_type TEXT NOT NULL DEFAULT 'SSH Key',
            password TEXT,
            key_path TEXT,
            key_passphrase TEXT,
            notes TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_used TEXT,
            favorite INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS groups (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT 'green',
            icon TEXT NOT NULL DEFAULT '+'
        );

        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            theme TEXT NOT NULL DEFAULT 'neon',
            default_port INTEGER NOT NULL DEFAULT 22,
            last_selected_server INTEGER,
            confirm_before_delete INTEGER NOT NULL DEFAULT 1,
            auto_ping INTEGER NOT NULL DEFAULT 0
        );

        INSERT OR IGNORE INTO settings (
            id, theme, default_port, confirm_before_delete, auto_ping
        ) VALUES (1, 'neon', 22, 1, 0);
        ",
    )?;
    Ok(())
}

fn load_settings(conn: &Connection) -> Result<Settings> {
    conn.query_row(
        "SELECT theme, default_port, last_selected_server, confirm_before_delete, auto_ping
         FROM settings WHERE id = 1",
        [],
        |row| {
            Ok(Settings {
                theme: row.get(0)?,
                default_port: row.get(1)?,
                last_selected_server: row.get(2)?,
                confirm_before_delete: row.get::<_, i64>(3)? != 0,
                auto_ping: row.get::<_, i64>(4)? != 0,
            })
        },
    )
    .map_err(Into::into)
}

fn load_groups(conn: &Connection) -> Result<Vec<Group>> {
    let mut stmt = conn.prepare("SELECT id, name, color, icon FROM groups ORDER BY name")?;
    let groups = stmt
        .query_map([], |row| {
            Ok(Group {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                icon: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(groups)
}

fn load_servers(conn: &Connection) -> Result<Vec<Server>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, host, username, port, group_name, auth_type, password, key_path,
                key_passphrase, notes, tags, created_at, updated_at, last_used, favorite
         FROM servers",
    )?;
    let servers = stmt
        .query_map([], |row| {
            Ok(Server {
                id: row.get(0)?,
                name: row.get(1)?,
                host: row.get(2)?,
                username: row.get(3)?,
                port: row.get(4)?,
                group_name: row.get(5)?,
                auth_type: row.get(6)?,
                password: row.get(7)?,
                key_path: row.get(8)?,
                key_passphrase: row.get(9)?,
                notes: row.get(10)?,
                tags: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
                last_used: row.get(14)?,
                favorite: row.get::<_, i64>(15)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(servers)
}

fn default_db_path() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(app_data) = env::var("APPDATA").or_else(|_| env::var("LOCALAPPDATA")) {
            return Ok(PathBuf::from(app_data).join(APP_NAME).join(DB_FILE));
        }
        if let Ok(user_profile) = env::var("USERPROFILE") {
            return Ok(PathBuf::from(user_profile)
                .join("AppData")
                .join("Roaming")
                .join(APP_NAME)
                .join(DB_FILE));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = env::var("HOME") {
            return Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_NAME)
                .join(DB_FILE));
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(data_home) = env::var("XDG_DATA_HOME") {
            return Ok(PathBuf::from(data_home).join(APP_NAME).join(DB_FILE));
        }
        if let Ok(home) = env::var("HOME") {
            return Ok(PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(APP_NAME)
                .join(DB_FILE));
        }
    }

    if let Ok(data_home) = env::var("XDG_DATA_HOME") {
        return Ok(PathBuf::from(data_home).join(APP_NAME).join(DB_FILE));
    }
    if let Ok(home) = env::var("HOME") {
        return Ok(PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_NAME)
            .join(DB_FILE));
    }
    Ok(env::current_dir()?.join(DB_FILE))
}

fn panel(title: &str) -> Block<'static> {
    Block::bordered()
        .title(format!(" {title} "))
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(theme::BORDER))
        .title_style(
            Style::default()
                .fg(theme::CYAN)
                .add_modifier(Modifier::BOLD),
        )
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

fn inset(area: Rect, horizontal: u16, vertical: u16) -> Rect {
    area.inner(Margin {
        horizontal,
        vertical,
    })
}

fn set_clamped_cursor(frame: &mut Frame, area: Rect, x: u16, y: u16) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let max_x = area.x + area.width.saturating_sub(1);
    let max_y = area.y + area.height.saturating_sub(1);
    frame.set_cursor_position(Position {
        x: x.clamp(area.x, max_x),
        y: y.clamp(area.y, max_y),
    });
}

fn visible_tail(value: &str, width: u16) -> String {
    let width = width as usize;
    if width == 0 {
        return String::new();
    }
    let chars = value.chars().collect::<Vec<_>>();
    let start = chars.len().saturating_sub(width);
    chars[start..].iter().collect()
}

fn visible_value_and_cursor(value: &str, cursor: usize, width: u16) -> (String, u16) {
    let width = width as usize;
    if width == 0 {
        return (String::new(), 0);
    }
    let chars = value.chars().collect::<Vec<_>>();
    let cursor = min(cursor, chars.len());
    let start = if chars.len() > width && cursor > width {
        cursor - width
    } else {
        0
    };
    let end = min(start + width, chars.len());
    let visible = chars[start..end].iter().collect();
    (visible, (cursor - start) as u16)
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn insert_char_at(value: &mut String, cursor: usize, ch: char) -> usize {
    let cursor = min(cursor, value.chars().count());
    let index = char_to_byte_index(value, cursor);
    value.insert(index, ch);
    cursor + 1
}

fn backspace_at(value: &mut String, cursor: usize) -> usize {
    if cursor == 0 {
        return 0;
    }
    let len = value.chars().count();
    let cursor = min(cursor, len);
    let start = char_to_byte_index(value, cursor - 1);
    let end = char_to_byte_index(value, cursor);
    value.replace_range(start..end, "");
    cursor - 1
}

fn delete_char_at(value: &mut String, cursor: usize) -> usize {
    let len = value.chars().count();
    if cursor >= len {
        return len;
    }
    let start = char_to_byte_index(value, cursor);
    let end = char_to_byte_index(value, cursor + 1);
    value.replace_range(start..end, "");
    cursor
}

fn visible_editor_lines(value: &str, width: u16, height: u16) -> Vec<String> {
    let width = width.max(1);
    let height = height.max(1) as usize;
    let mut rendered = Vec::new();
    for source_line in value.split('\n') {
        if source_line.is_empty() {
            rendered.push(String::new());
            continue;
        }
        let chars = source_line.chars().collect::<Vec<_>>();
        for chunk in chars.chunks(width as usize) {
            rendered.push(chunk.iter().collect());
        }
    }
    if rendered.is_empty() {
        rendered.push(String::new());
    }
    let start = rendered.len().saturating_sub(height);
    rendered[start..].to_vec()
}

fn editor_cursor(value: &str, area: Rect, visible_lines: usize) -> (u16, u16) {
    let last_line = value.split('\n').next_back().unwrap_or("");
    let width = area.width.max(1);
    let visible_tail = visible_tail(last_line, width);
    let y = area.y + visible_lines.saturating_sub(1) as u16;
    (area.x + visible_tail.len() as u16, y)
}

fn shortcut_line<'a>(key: &'a str, label: &'a str) -> Line<'a> {
    Line::from(vec![
        "[".into(),
        key.fg(theme::CYAN).bold(),
        "] ".into(),
        label.into(),
    ])
}

fn detail_line<'a>(label: &'a str, value: &'a str) -> Line<'a> {
    Line::from(vec![
        format!("  {label}: ").fg(theme::TEXT),
        value.fg(theme::MUTED),
    ])
}

fn option_span(label: &str, selected: bool) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        if selected {
            Style::default()
                .fg(Color::White)
                .bg(theme::PURPLE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::MUTED)
        },
    )
}

fn parsed_tags(tags: &str) -> Vec<&str> {
    tags.split([',', ' '])
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn tag_chip_lines(tags: &str, max_width: usize) -> Vec<Line<'static>> {
    let tags = parsed_tags(tags);
    if tags.is_empty() {
        return vec![Line::from("No tags available yet.").fg(theme::MUTED)];
    }

    let palette = [
        theme::PURPLE,
        theme::CYAN,
        theme::GREEN,
        theme::YELLOW,
        Color::Blue,
        Color::Magenta,
    ];
    let mut lines = Vec::new();
    let mut spans = Vec::new();
    let mut used = 0usize;

    for (index, tag) in tags.into_iter().enumerate() {
        let chip_width = tag.len() + 3;
        if !spans.is_empty() && used + chip_width > max_width {
            lines.push(Line::from(spans));
            spans = Vec::new();
            used = 0;
        }
        let bg = palette[index % palette.len()];
        spans.push(Span::styled(
            format!(" {tag} "),
            Style::default().fg(Color::White).bg(bg),
        ));
        spans.push(Span::raw(" "));
        used += chip_width;
    }

    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

fn empty_to_none(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn or_dash(value: &str) -> &str {
    if value.trim().is_empty() { "--" } else { value }
}

fn color_from_name(name: &str) -> Color {
    match name.to_lowercase().as_str() {
        "green" => theme::GREEN,
        "yellow" => theme::YELLOW,
        "red" => theme::RED,
        "purple" => theme::PURPLE,
        "blue" => Color::Blue,
        _ => theme::CYAN,
    }
}

fn relative_time(value: Option<&str>) -> String {
    if value.is_some() {
        "recent".to_string()
    } else {
        "--".to_string()
    }
}

fn copy_to_clipboard(value: &str) -> Result<()> {
    let commands = [
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["--clipboard", "--input"]),
        ("pbcopy", vec![]),
    ];
    for (cmd, args) in commands {
        let available = Command::new(cmd)
            .arg("--version")
            .output()
            .or_else(|_| Command::new(cmd).arg("-version").output())
            .is_ok();
        if !available && cmd != "pbcopy" {
            continue;
        }
        let mut child = Command::new(cmd)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .spawn();
        if let Ok(ref mut child) = child {
            if let Some(stdin) = &mut child.stdin {
                use std::io::Write;
                stdin.write_all(value.as_bytes())?;
            }
            if child.wait()?.success() {
                return Ok(());
            }
        }
    }
    Err(eyre!("no clipboard command available"))
}

impl Widget for &App {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new("russhx")
            .style(Style::default().fg(theme::CYAN))
            .render(area, buf);
    }
}

pub fn run() -> Result<()> {
    let mut app = App::new()?;
    ratatui::run(|terminal| app.run(terminal))?;
    if let Some(launch) = app.take_pending_ssh() {
        launch_ssh(launch)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_sqlite_schema_and_defaults() {
        let db_path = env::temp_dir().join(format!(
            "russhx-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app = App::with_db_path(db_path.clone()).unwrap();
        let settings = load_settings(&app.conn).unwrap();
        assert_eq!(settings.default_port, 22);
        assert!(app.servers.is_empty());
        assert_eq!(
            app.status.as_ref().map(|(_, message)| message.as_str()),
            Some("Welcome to russhx. Add a server to get started.")
        );
        drop(app);
        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn ssh_command_uses_key_when_present() {
        let server = Server {
            id: 1,
            name: "prod".to_string(),
            host: "10.0.0.1".to_string(),
            username: "ubuntu".to_string(),
            port: 2222,
            group_name: "Production".to_string(),
            auth_type: "SSH Key".to_string(),
            password: None,
            key_path: Some("~/.ssh/prod.pem".to_string()),
            key_passphrase: None,
            notes: String::new(),
            tags: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            last_used: None,
            favorite: false,
        };
        assert_eq!(
            ssh_command(&server),
            "ssh -i ~/.ssh/prod.pem ubuntu@10.0.0.1 -p 2222"
        );
    }

    #[test]
    fn search_filters_server_fields() {
        let db_path = env::temp_dir().join(format!(
            "russhx-search-test-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App::with_db_path(db_path.clone()).unwrap();
        app.conn
            .execute(
                "INSERT INTO servers (name, host, username, port, group_name, auth_type, tags)
                 VALUES ('prod-web-01', '10.0.0.1', 'ubuntu', 22, 'Production', 'SSH Key', 'web,nginx')",
                [],
            )
            .unwrap();
        app.reload().unwrap();
        app.search_query = "nginx".to_string();
        assert_eq!(app.filtered_servers().len(), 1);
        app.search_query = "missing".to_string();
        assert_eq!(app.filtered_servers().len(), 0);
        drop(app);
        let _ = fs::remove_file(db_path);
    }
}
