//! `dupfinder tui` subcommand implementation.
//!
//! Interactive Terminal User Interface built with Ratatui and Crossterm.
//! Provides dual-pane navigation, visual diff and text previews, keyboard shortcuts
//! for marking actions ([D]elete, [L]ink), and secure non-shell file opening.

use anyhow::{bail, Context, Result};
use clap::Args;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use dupfinder_core::clean::{execute_clean_plan, CleanAction, DeletionMethod, RemediationStrategy};
use dupfinder_core::progress::SilentProgress;
use dupfinder_core::types::{
    CacheConfig, DuplicateGroup, FeatureFlags, FilterConfig, ScanConfig, ScanReport,
};

use crate::output::CliProgressHandler;

/// Arguments for the `tui` subcommand.
#[derive(Args, Debug)]
pub struct TuiArgs {
    /// One or more directories to scan and inspect.
    #[arg(required = true)]
    pub paths: Vec<PathBuf>,

    /// Minimum file size to consider (e.g., 1024, 1KB, 1MB).
    #[arg(long, default_value = "1")]
    pub min_size: String,

    /// Glob pattern(s) to exclude files (repeatable).
    #[arg(long)]
    pub exclude: Vec<String>,

    /// Directory name(s) to skip (repeatable).
    #[arg(long)]
    pub exclude_dir: Vec<String>,

    /// Include hidden files/directories (dotfiles).
    #[arg(long)]
    pub include_hidden: bool,

    /// Built-in ignore preset: default, build, deps, jars, minimal, none.
    #[arg(long, value_name = "PRESET")]
    pub exclude_preset: Option<String>,

    /// Disable built-in default ignore presets.
    #[arg(long)]
    pub no_default_ignores: bool,

    /// Explicit custom ignore file to load (repeatable).
    #[arg(long = "ignore-file", value_name = "PATH")]
    pub ignore_files: Vec<PathBuf>,

    /// Scan JAR and archive files.
    #[arg(long)]
    pub include_jars: bool,

    /// Maximum recursion depth.
    #[arg(long, short = 'd')]
    pub depth: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusPane {
    GroupsList,
    DuplicatesList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkedAction {
    Delete,
    Hardlink,
}

struct App {
    report: ScanReport,
    selected_group_idx: usize,
    selected_dup_idx: usize,
    focus: FocusPane,
    marked_actions: HashMap<PathBuf, MarkedAction>,
    show_help: bool,
    show_confirm: bool,
    status_message: String,
    preview_content: String,
}

impl App {
    fn new(report: ScanReport) -> Self {
        let mut app = Self {
            report,
            selected_group_idx: 0,
            selected_dup_idx: 0,
            focus: FocusPane::GroupsList,
            marked_actions: HashMap::new(),
            show_help: false,
            show_confirm: false,
            status_message: "Ready. Press [?] for help, [Enter] to apply marked actions."
                .to_string(),
            preview_content: String::new(),
        };
        app.update_preview();
        app
    }

    fn current_group(&self) -> Option<&DuplicateGroup> {
        self.report.duplicates.groups.get(self.selected_group_idx)
    }

    fn current_duplicate_path(&self) -> Option<&PathBuf> {
        if let Some(group) = self.current_group() {
            if self.selected_dup_idx == 0 {
                Some(&group.original.path)
            } else {
                group
                    .duplicates
                    .get(self.selected_dup_idx - 1)
                    .map(|e| &e.path)
            }
        } else {
            None
        }
    }

    fn next_group(&mut self) {
        if !self.report.duplicates.groups.is_empty() {
            self.selected_group_idx =
                (self.selected_group_idx + 1) % self.report.duplicates.groups.len();
            self.selected_dup_idx = 0;
            self.update_preview();
        }
    }

    fn prev_group(&mut self) {
        if !self.report.duplicates.groups.is_empty() {
            if self.selected_group_idx == 0 {
                self.selected_group_idx = self.report.duplicates.groups.len() - 1;
            } else {
                self.selected_group_idx -= 1;
            }
            self.selected_dup_idx = 0;
            self.update_preview();
        }
    }

    fn next_item(&mut self) {
        if let Some(group) = self.current_group() {
            let total_items = group.duplicates.len() + 1; // original + duplicates
            if total_items > 0 {
                self.selected_dup_idx = (self.selected_dup_idx + 1) % total_items;
                self.update_preview();
            }
        }
    }

    fn prev_item(&mut self) {
        if let Some(group) = self.current_group() {
            let total_items = group.duplicates.len() + 1;
            if total_items > 0 {
                if self.selected_dup_idx == 0 {
                    self.selected_dup_idx = total_items - 1;
                } else {
                    self.selected_dup_idx -= 1;
                }
                self.update_preview();
            }
        }
    }

    fn toggle_mark(&mut self, action: MarkedAction) {
        if let Some(group) = self.current_group() {
            if self.selected_dup_idx == 0 {
                self.status_message =
                    "Cannot mark canonical original file for deletion/linking!".to_string();
                return;
            }
            if let Some(dup) = group.duplicates.get(self.selected_dup_idx - 1) {
                let path = dup.path.clone();
                if let Some(existing) = self.marked_actions.get(&path) {
                    if *existing == action {
                        self.marked_actions.remove(&path);
                        self.status_message = format!("Unmarked '{}'", path.display());
                    } else {
                        self.marked_actions.insert(path.clone(), action);
                        self.status_message =
                            format!("Marked '{}' for {:?}", path.display(), action);
                    }
                } else {
                    self.marked_actions.insert(path.clone(), action);
                    self.status_message = format!("Marked '{}' for {:?}", path.display(), action);
                }
            }
        }
    }

    fn open_in_viewer(&mut self) {
        if let Some(path) = self.current_duplicate_path() {
            let path_clone = path.clone();
            #[cfg(target_os = "macos")]
            let res = Command::new("open").arg(&path_clone).spawn();
            #[cfg(target_os = "linux")]
            let res = Command::new("xdg-open").arg(&path_clone).spawn();
            #[cfg(target_os = "windows")]
            let res = Command::new("explorer").arg(&path_clone).spawn();
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            let res = Ok(());

            match res {
                Ok(_) => {
                    self.status_message =
                        format!("Opened '{}' in default application.", path_clone.display())
                }
                Err(e) => self.status_message = format!("Failed to open file: {}", e),
            }
        }
    }

    fn update_preview(&mut self) {
        if let Some(path) = self.current_duplicate_path() {
            if !path.exists() {
                self.preview_content = format!("File does not exist: {}", path.display());
                return;
            }

            match File::open(path) {
                Ok(mut file) => {
                    let mut buffer = [0u8; 1024];
                    match file.read(&mut buffer) {
                        Ok(bytes_read) => {
                            if bytes_read == 0 {
                                self.preview_content = "[Empty 0-byte file]".to_string();
                            } else if let Ok(text) = std::str::from_utf8(&buffer[..bytes_read]) {
                                self.preview_content =
                                    text.lines().take(20).collect::<Vec<_>>().join("\n");
                            } else {
                                // Hex dump preview for binary files
                                let mut hex_lines = Vec::new();
                                for (i, chunk) in
                                    buffer[..bytes_read.min(256)].chunks(16).enumerate()
                                {
                                    let hex = chunk
                                        .iter()
                                        .map(|b| format!("{:02x}", b))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    let ascii: String = chunk
                                        .iter()
                                        .map(|&b| {
                                            if b.is_ascii_graphic() || b == b' ' {
                                                b as char
                                            } else {
                                                '.'
                                            }
                                        })
                                        .collect();
                                    hex_lines.push(format!(
                                        "{:04x}: {:<48} |{}|",
                                        i * 16,
                                        hex,
                                        ascii
                                    ));
                                }
                                self.preview_content = format!(
                                    "[Binary File - Hex Preview]\n{}",
                                    hex_lines.join("\n")
                                );
                            }
                        }
                        Err(e) => {
                            self.preview_content = format!("Error reading file: {}", e);
                        }
                    }
                }
                Err(e) => {
                    self.preview_content = format!("Could not open file: {}", e);
                }
            }
        } else {
            self.preview_content = "No file selected.".to_string();
        }
    }

    fn execute_marked(&mut self) -> Result<String> {
        let mut files_deleted = 0;
        let mut files_linked = 0;
        let mut bytes_saved = 0;
        let progress = SilentProgress;

        for group in &self.report.duplicates.groups {
            let original = &group.original.path;
            for dup in &group.duplicates {
                if let Some(action) = self.marked_actions.get(&dup.path) {
                    match action {
                        MarkedAction::Delete => {
                            let filtered_plan = dupfinder_core::clean::CleanPlan {
                                actions: vec![CleanAction::DeleteDuplicate {
                                    path: dup.path.clone(),
                                    size: dup.size,
                                    original: original.clone(),
                                }],
                                total_files_to_remediate: 1,
                                total_dirs_to_remove: 0,
                                total_bytes_reclaimable: dup.size,
                                already_linked_count: 0,
                                blocked_count: 0,
                                skipped_count: 0,
                            };
                            let res = execute_clean_plan(
                                &filtered_plan,
                                RemediationStrategy::Delete(DeletionMethod::Trash),
                                false,
                                &progress,
                            )?;
                            if res.succeeded_files > 0 {
                                files_deleted += 1;
                                bytes_saved += dup.size;
                            }
                        }
                        MarkedAction::Hardlink => {
                            let filtered_plan = dupfinder_core::clean::CleanPlan {
                                actions: vec![CleanAction::HardlinkDuplicate {
                                    path: dup.path.clone(),
                                    size: dup.size,
                                    original: original.clone(),
                                }],
                                total_files_to_remediate: 1,
                                total_dirs_to_remove: 0,
                                total_bytes_reclaimable: dup.size,
                                already_linked_count: 0,
                                blocked_count: 0,
                                skipped_count: 0,
                            };
                            let res = execute_clean_plan(
                                &filtered_plan,
                                RemediationStrategy::Hardlink,
                                false,
                                &progress,
                            )?;
                            if res.succeeded_files > 0 {
                                files_linked += 1;
                                bytes_saved += dup.size;
                            }
                        }
                    }
                }
            }
        }

        self.marked_actions.clear();
        Ok(format!(
            "Success! Trashed {} files, Hardlinked {} files. Reclaimed {}.",
            files_deleted,
            files_linked,
            format_bytes(bytes_saved)
        ))
    }
}

/// Run the `tui` subcommand.
pub fn run(args: TuiArgs) -> Result<()> {
    // 1. Configure scan parameters
    let min_size_bytes = parse_size_str(&args.min_size)
        .with_context(|| format!("Invalid --min-size value '{}'", args.min_size))?;

    let preset = if args.no_default_ignores {
        None
    } else if let Some(ref preset_name) = args.exclude_preset {
        match dupfinder_core::ignore::IgnorePreset::from_str_name(preset_name) {
            Some(p) => Some(p),
            None => anyhow::bail!(
                "Invalid preset: '{}'. Valid presets: default, build, deps, jars, minimal, none",
                preset_name
            ),
        }
    } else {
        Some(dupfinder_core::ignore::IgnorePreset::Default)
    };

    let filter_config = FilterConfig {
        min_size: min_size_bytes,
        exclude_patterns: args.exclude,
        exclude_dirs: args.exclude_dir,
        preset,
        include_jars: args.include_jars,
        use_global_ignore: true,
        use_project_ignore: true,
        custom_ignore_files: args.ignore_files,
    };

    let scan_config = ScanConfig {
        paths: args.paths.clone(),
        max_depth: args.depth,
        include_hidden: args.include_hidden,
        features: FeatureFlags {
            duplicates: true,
            empty_files: false,
            empty_dirs: false,
            broken_links: false,
        },
        filters: filter_config,
        cache_config: CacheConfig::default(),
    };

    // 2. Perform initial scan
    let progress_handler = CliProgressHandler::new(false);
    let report = dupfinder_core::scan(scan_config, &progress_handler)
        .context("Failed during initial scan")?;

    if report.duplicates.groups.is_empty() {
        println!("\n✨ No duplicate files detected in scanned directories!");
        return Ok(());
    }

    // 3. Setup Terminal
    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .context("Failed to enter alternate terminal screen")?;

    // Setup panic hook to always restore terminal
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        default_panic(info);
    }));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to initialize ratatui terminal")?;

    let mut app = App::new(report);
    let run_res = run_app(&mut terminal, &mut app);

    // 4. Restore Terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = run_res {
        eprintln!("TUI Error: {}", e);
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if app.show_help {
                        match key.code {
                            KeyCode::Esc
                            | KeyCode::Char('?')
                            | KeyCode::Char('q')
                            | KeyCode::Enter => {
                                app.show_help = false;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.show_confirm {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                                match app.execute_marked() {
                                    Ok(msg) => app.status_message = msg,
                                    Err(e) => {
                                        app.status_message = format!("Execution failed: {}", e)
                                    }
                                }
                                app.show_confirm = false;
                            }
                            KeyCode::Esc
                            | KeyCode::Char('n')
                            | KeyCode::Char('N')
                            | KeyCode::Char('q') => {
                                app.show_confirm = false;
                                app.status_message = "Execution cancelled.".to_string();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Char('?') => app.show_help = true,
                        KeyCode::Tab => {
                            app.focus = match app.focus {
                                FocusPane::GroupsList => FocusPane::DuplicatesList,
                                FocusPane::DuplicatesList => FocusPane::GroupsList,
                            };
                        }
                        KeyCode::Up | KeyCode::Char('k') => match app.focus {
                            FocusPane::GroupsList => app.prev_group(),
                            FocusPane::DuplicatesList => app.prev_item(),
                        },
                        KeyCode::Down | KeyCode::Char('j') => match app.focus {
                            FocusPane::GroupsList => app.next_group(),
                            FocusPane::DuplicatesList => app.next_item(),
                        },
                        KeyCode::Char('d') | KeyCode::Char('D') => {
                            app.toggle_mark(MarkedAction::Delete);
                        }
                        KeyCode::Char('l') | KeyCode::Char('L') => {
                            app.toggle_mark(MarkedAction::Hardlink);
                        }
                        KeyCode::Char('o') | KeyCode::Char('O') => {
                            app.open_in_viewer();
                        }
                        KeyCode::Enter => {
                            if !app.marked_actions.is_empty() {
                                app.show_confirm = true;
                            } else {
                                app.status_message = "No items marked! Use [D] to mark for deletion or [L] for hardlink.".to_string();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    // 1. Header
    let marked_count = app.marked_actions.len();
    let header_text = format!(
        " 🔍 dupfinder TUI Inspector  │  Groups: {}  │  Wasted Space: {}  │  Marked: {}",
        app.report.duplicates.total_groups,
        format_bytes(app.report.duplicates.reclaimable_bytes),
        marked_count
    );
    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(header, chunks[0]);

    // 2. Dual Pane Layout
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // ── Left: Groups List ──
    let group_items: Vec<ListItem> = app
        .report
        .duplicates
        .groups
        .iter()
        .enumerate()
        .map(|(idx, g)| {
            let is_selected = idx == app.selected_group_idx;
            let group_num = idx + 1;
            let file_name = g
                .original
                .path
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            let total_copies = g.duplicates.len() + 1;

            let line = Line::from(vec![
                Span::styled(
                    format!("#{:02} ", group_num),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:<20} ", file_name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("({} copies, {})", total_copies, format_bytes(g.size)),
                    Style::default().fg(Color::Gray),
                ),
            ]);

            let style = if is_selected {
                Style::default()
                    .bg(Color::Rgb(30, 45, 75))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(line).style(style)
        })
        .collect();

    let groups_border_style = if app.focus == FocusPane::GroupsList {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let groups_list = List::new(group_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(groups_border_style)
            .title(" Duplicate Groups (Press [Tab] to switch) "),
    );
    f.render_widget(groups_list, body_chunks[0]);

    // ── Right: Group Details & Preview ──
    let details_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(body_chunks[1]);

    if let Some(group) = app.current_group() {
        let mut item_lines = Vec::new();

        // 1. Original file entry
        let orig_is_selected = app.selected_dup_idx == 0;
        let orig_line = Line::from(vec![
            Span::styled(
                " [ORIGINAL] ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{}", group.original.path.display())),
        ]);
        let orig_style = if orig_is_selected && app.focus == FocusPane::DuplicatesList {
            Style::default().bg(Color::Rgb(30, 60, 40)).fg(Color::White)
        } else {
            Style::default()
        };
        item_lines.push(ListItem::new(orig_line).style(orig_style));

        // 2. Duplicate copies
        for (i, dup) in group.duplicates.iter().enumerate() {
            let is_selected = app.selected_dup_idx == i + 1;
            let mark_badge = match app.marked_actions.get(&dup.path) {
                Some(MarkedAction::Delete) => Span::styled(
                    " [DELETE]   ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Some(MarkedAction::Hardlink) => Span::styled(
                    " [HARDLINK] ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                None => Span::styled(" [KEEP]     ", Style::default().fg(Color::DarkGray)),
            };

            let dup_line = Line::from(vec![
                mark_badge,
                Span::raw(format!("{}", dup.path.display())),
            ]);

            let item_style = if is_selected && app.focus == FocusPane::DuplicatesList {
                Style::default().bg(Color::Rgb(50, 40, 60)).fg(Color::White)
            } else {
                Style::default()
            };

            item_lines.push(ListItem::new(dup_line).style(item_style));
        }

        let items_border_style = if app.focus == FocusPane::DuplicatesList {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let items_list = List::new(item_lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(items_border_style)
                .title(format!(
                    " Group Files (Hash: {}...) ",
                    &group.hash[..8.min(group.hash.len())]
                )),
        );
        f.render_widget(items_list, details_chunks[0]);

        // 3. File Inspector / Content Preview Box
        let preview_widget = Paragraph::new(app.preview_content.as_str())
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Content / Hex Preview "),
            );
        f.render_widget(preview_widget, details_chunks[1]);
    }

    // 3. Footer / Keybindings Status
    let footer_text = format!(
        " [↑/↓/j/k] Navigate  [Tab] Pane  [D] Mark Delete  [L] Mark Hardlink  [O] Open Viewer  [Enter] Apply  [?] Help  [q] Quit │ {}",
        app.status_message
    );
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(footer, chunks[2]);

    // 4. Modals
    if app.show_help {
        render_help_modal(f);
    }
    if app.show_confirm {
        render_confirm_modal(f, app.marked_actions.len());
    }
}

fn render_help_modal(f: &mut Frame) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);

    let help_text = vec![
        Line::from(Span::styled(
            " Keyboard Controls & Shortcuts ",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  [↑] / [k]       : Move cursor up"),
        Line::from("  [↓] / [j]       : Move cursor down"),
        Line::from("  [Tab]           : Switch focus between Groups list and Duplicates list"),
        Line::from("  [D]             : Toggle mark for Safe Deletion (OS Trash)"),
        Line::from("  [L]             : Toggle mark for Hardlink Deduplication"),
        Line::from("  [O]             : Open selected file in external system default viewer"),
        Line::from("  [Enter]         : Execute marked actions with confirmation dialog"),
        Line::from("  [?]             : Toggle this help dialog"),
        Line::from("  [q] / [Esc]     : Quit inspector / close dialog"),
        Line::from(""),
        Line::from(Span::styled(
            "Press [Esc], [q], or [Enter] to return.",
            Style::default().fg(Color::Cyan),
        )),
    ];

    let modal = Paragraph::new(help_text).alignment(Alignment::Left).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Help & Keybindings "),
    );

    f.render_widget(modal, area);
}

fn render_confirm_modal(f: &mut Frame, marked_count: usize) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);

    let confirm_text = vec![
        Line::from(Span::styled(
            " Confirm Remediation ",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!(
            "You have marked {} file(s) for remediation.",
            marked_count
        )),
        Line::from("Deleted files will be moved to the OS Trash safely."),
        Line::from("Hardlinked files will be replaced with atomic hardlinks to originals."),
        Line::from(""),
        Line::from(Span::styled(
            "Apply marked changes now? [y/N]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let modal = Paragraph::new(confirm_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Execute Confirmation "),
        );

    f.render_widget(modal, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn parse_size_str(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }

    let (num_part, unit_part) = match s.find(|c: char| c.is_alphabetic()) {
        Some(idx) => (&s[..idx], &s[idx..]),
        None => (s, ""),
    };

    let number: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number '{}' in size", num_part))?;

    let multiplier: u64 = match unit_part.to_lowercase().as_str() {
        "" | "b" | "bytes" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        "t" | "tb" | "tib" => 1024 * 1024 * 1024 * 1024,
        _ => bail!("Unknown size unit '{}'. Use B, KB, MB, GB, TB", unit_part),
    };

    Ok((number * multiplier as f64) as u64)
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dupfinder_core::types::{DuplicateReport, FileEntry, ScanInfo};
    use std::fs;
    use std::time::SystemTime;
    use tempfile::tempdir;

    fn mock_scan_report(groups: Vec<DuplicateGroup>) -> ScanReport {
        let total_groups = groups.len();
        let total_redundant = groups.iter().map(|g| g.duplicates.len()).sum();
        let reclaimable = groups
            .iter()
            .map(|g| g.size * g.duplicates.len() as u64)
            .sum();

        ScanReport {
            version: "0.1.0".to_string(),
            scan_info: ScanInfo {
                paths: vec![PathBuf::from("/test")],
                timestamp: "2026-08-14T00:00:00Z".to_string(),
                duration_secs: 1.0,
                files_scanned: 10,
                dirs_scanned: 2,
            },
            duplicates: DuplicateReport {
                total_groups,
                total_redundant_files: total_redundant,
                reclaimable_bytes: reclaimable,
                groups,
            },
            empty_files: vec![],
            empty_dirs: vec![],
            broken_symlinks: vec![],
            cache_stats: Default::default(),
        }
    }

    #[test]
    fn test_app_navigation() {
        let group1 = DuplicateGroup {
            hash: "hash1".to_string(),
            size: 100,
            original: FileEntry {
                path: PathBuf::from("orig1.txt"),
                size: 100,
                mtime: SystemTime::UNIX_EPOCH,
            },
            duplicates: vec![FileEntry {
                path: PathBuf::from("dup1.txt"),
                size: 100,
                mtime: SystemTime::UNIX_EPOCH,
            }],
        };
        let group2 = DuplicateGroup {
            hash: "hash2".to_string(),
            size: 200,
            original: FileEntry {
                path: PathBuf::from("orig2.txt"),
                size: 200,
                mtime: SystemTime::UNIX_EPOCH,
            },
            duplicates: vec![FileEntry {
                path: PathBuf::from("dup2.txt"),
                size: 200,
                mtime: SystemTime::UNIX_EPOCH,
            }],
        };

        let mut app = App::new(mock_scan_report(vec![group1, group2]));
        assert_eq!(app.selected_group_idx, 0);

        app.next_group();
        assert_eq!(app.selected_group_idx, 1);

        app.next_group(); // Wrap around
        assert_eq!(app.selected_group_idx, 0);

        app.prev_group(); // Wrap backwards
        assert_eq!(app.selected_group_idx, 1);
    }

    #[test]
    fn test_app_item_navigation_and_original_protection() {
        let group = DuplicateGroup {
            hash: "hash1".to_string(),
            size: 100,
            original: FileEntry {
                path: PathBuf::from("orig.txt"),
                size: 100,
                mtime: SystemTime::UNIX_EPOCH,
            },
            duplicates: vec![
                FileEntry {
                    path: PathBuf::from("dup1.txt"),
                    size: 100,
                    mtime: SystemTime::UNIX_EPOCH,
                },
                FileEntry {
                    path: PathBuf::from("dup2.txt"),
                    size: 100,
                    mtime: SystemTime::UNIX_EPOCH,
                },
            ],
        };

        let mut app = App::new(mock_scan_report(vec![group]));
        assert_eq!(app.selected_dup_idx, 0);

        // Attempting to mark original must be blocked!
        app.toggle_mark(MarkedAction::Delete);
        assert!(app.marked_actions.is_empty());
        assert!(app
            .status_message
            .contains("Cannot mark canonical original"));

        // Navigate to first duplicate copy
        app.next_item();
        assert_eq!(app.selected_dup_idx, 1);

        // Mark first duplicate for deletion
        app.toggle_mark(MarkedAction::Delete);
        assert_eq!(app.marked_actions.len(), 1);
        assert_eq!(
            app.marked_actions.get(&PathBuf::from("dup1.txt")),
            Some(&MarkedAction::Delete)
        );

        // Change to Hardlink
        app.toggle_mark(MarkedAction::Hardlink);
        assert_eq!(
            app.marked_actions.get(&PathBuf::from("dup1.txt")),
            Some(&MarkedAction::Hardlink)
        );

        // Unmark
        app.toggle_mark(MarkedAction::Hardlink);
        assert!(app.marked_actions.is_empty());
    }

    #[test]
    fn test_app_preview_text_and_binary() {
        let dir = tempdir().unwrap();
        let text_path = dir.path().join("test.txt");
        let bin_path = dir.path().join("test.bin");
        let empty_path = dir.path().join("empty.txt");

        fs::write(&text_path, b"Line 1\nLine 2\nLine 3").unwrap();
        fs::write(&bin_path, [0x00, 0xFF, 0xFE, 0x00, 0x12, 0x34]).unwrap();
        fs::write(&empty_path, b"").unwrap();

        let group = DuplicateGroup {
            hash: "testhash".to_string(),
            size: 10,
            original: FileEntry {
                path: text_path,
                size: 10,
                mtime: SystemTime::UNIX_EPOCH,
            },
            duplicates: vec![
                FileEntry {
                    path: bin_path,
                    size: 6,
                    mtime: SystemTime::UNIX_EPOCH,
                },
                FileEntry {
                    path: empty_path,
                    size: 0,
                    mtime: SystemTime::UNIX_EPOCH,
                },
            ],
        };

        let mut app = App::new(mock_scan_report(vec![group]));

        // Text file preview
        app.selected_dup_idx = 0;
        app.update_preview();
        assert!(app.preview_content.contains("Line 1"));

        // Binary file preview
        app.selected_dup_idx = 1;
        app.update_preview();
        assert!(app.preview_content.contains("[Binary File - Hex Preview]"));

        // Empty file preview
        app.selected_dup_idx = 2;
        app.update_preview();
        assert!(app.preview_content.contains("[Empty 0-byte file]"));
    }

    #[test]
    fn test_centered_rect_dimensions() {
        let area = Rect::new(0, 0, 100, 100);
        let popup = centered_rect(50, 50, area);
        assert_eq!(popup.width, 50);
        assert_eq!(popup.height, 50);
        assert_eq!(popup.x, 25);
        assert_eq!(popup.y, 25);
    }

    #[test]
    fn test_parse_size_str_and_format_bytes() {
        assert_eq!(parse_size_str("1024").unwrap(), 1024);
        assert_eq!(parse_size_str("1KB").unwrap(), 1024);
        assert_eq!(parse_size_str("5MB").unwrap(), 5 * 1024 * 1024);
        assert_eq!(parse_size_str("2GB").unwrap(), 2 * 1024 * 1024 * 1024);
        assert!(parse_size_str("invalid").is_err());

        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_app_execute_marked_actions() {
        let dir = tempdir().unwrap();
        let orig = dir.path().join("orig.txt");
        let dup1 = dir.path().join("dup1.txt");
        let dup2 = dir.path().join("dup2.txt");

        fs::write(&orig, b"content").unwrap();
        fs::write(&dup1, b"content").unwrap();
        fs::write(&dup2, b"content").unwrap();

        let group = DuplicateGroup {
            hash: "h".to_string(),
            size: 7,
            original: FileEntry {
                path: orig.clone(),
                size: 7,
                mtime: SystemTime::UNIX_EPOCH,
            },
            duplicates: vec![
                FileEntry {
                    path: dup1.clone(),
                    size: 7,
                    mtime: SystemTime::UNIX_EPOCH,
                },
                FileEntry {
                    path: dup2.clone(),
                    size: 7,
                    mtime: SystemTime::UNIX_EPOCH,
                },
            ],
        };

        let mut app = App::new(mock_scan_report(vec![group]));

        // Mark dup1 for Delete and dup2 for Hardlink
        app.marked_actions
            .insert(dup1.clone(), MarkedAction::Delete);
        app.marked_actions
            .insert(dup2.clone(), MarkedAction::Hardlink);

        let res = app.execute_marked();
        assert!(res.is_ok());

        assert!(orig.exists(), "Original file must exist");
        assert!(!dup1.exists(), "Deleted duplicate must be removed");
        assert!(dup2.exists(), "Hardlinked duplicate must exist");
        assert!(
            dupfinder_core::clean::are_same_inode(&orig, &dup2),
            "dup2 must share inode with original"
        );
    }
}
