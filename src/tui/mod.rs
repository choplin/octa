//! Read-only terminal browser for issues.

use crate::domain::issue::IssueDetail;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io::{self, Stdout};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Focus {
    List,
    Detail,
}

pub(crate) struct App {
    issues: Vec<IssueDetail>,
    selected: usize,
    list_offset: usize,
    detail_scroll: u16,
    max_detail_scroll: u16,
    focus: Focus,
    quit: bool,
}

impl App {
    fn new(issues: Vec<IssueDetail>) -> Self {
        Self {
            issues,
            selected: 0,
            list_offset: 0,
            detail_scroll: 0,
            max_detail_scroll: 0,
            focus: Focus::List,
            quit: false,
        }
    }

    fn selected_issue(&self) -> Option<&IssueDetail> {
        self.issues.get(self.selected)
    }

    fn select_next(&mut self) {
        if !self.issues.is_empty() {
            self.selected = (self.selected + 1).min(self.issues.len() - 1);
            self.detail_scroll = 0;
        }
    }

    fn select_previous(&mut self) {
        if !self.issues.is_empty() {
            self.selected = self.selected.saturating_sub(1);
            self.detail_scroll = 0;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    Focus::List => Focus::Detail,
                    Focus::Detail => Focus::List,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::List => self.select_next(),
                Focus::Detail => {
                    self.detail_scroll = self
                        .detail_scroll
                        .saturating_add(1)
                        .min(self.max_detail_scroll);
                }
            },
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::List => self.select_previous(),
                Focus::Detail => {
                    self.detail_scroll = self.detail_scroll.saturating_sub(1);
                }
            },
            KeyCode::PageDown if self.focus == Focus::Detail => {
                self.detail_scroll = self
                    .detail_scroll
                    .saturating_add(10)
                    .min(self.max_detail_scroll);
            }
            KeyCode::PageUp if self.focus == Focus::Detail => {
                self.detail_scroll = self.detail_scroll.saturating_sub(10);
            }
            KeyCode::Home => match self.focus {
                Focus::List => {
                    self.selected = 0;
                    self.detail_scroll = 0;
                }
                Focus::Detail => self.detail_scroll = 0,
            },
            KeyCode::End => match self.focus {
                Focus::List if !self.issues.is_empty() => {
                    self.selected = self.issues.len() - 1;
                    self.detail_scroll = 0;
                }
                Focus::Detail => self.detail_scroll = self.max_detail_scroll,
                Focus::List => {}
            },
            _ => {}
        }
    }
}

pub(crate) fn run(issues: Vec<IssueDetail>) -> Result<()> {
    let mut session = TerminalSession::enter()?;
    let result = run_terminal(io::stdout(), issues);
    let cleanup_result = session.restore();
    result?;
    cleanup_result
}

/// Restores the user's terminal both on ordinary errors and while unwinding
/// from a panic in rendering/event handling.
struct TerminalSession {
    active: bool,
}

impl TerminalSession {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        if let Err(error) = execute!(io::stdout(), EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        Ok(Self { active: true })
    }

    fn restore(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let raw_result = disable_raw_mode();
        let screen_result = execute!(io::stdout(), LeaveAlternateScreen);
        raw_result?;
        screen_result?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn run_terminal(stdout: Stdout, issues: Vec<IssueDetail>) -> Result<()> {
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    let mut app = App::new(issues);
    while !app.quit {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if let Event::Key(key) = event::read()? {
            app.handle_key(key);
        }
    }
    terminal.show_cursor()?;
    Ok(())
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    if area.width < 20 || area.height < 5 {
        frame.render_widget(
            Paragraph::new("octa issue tui\nterminal too small")
                .block(Block::default().borders(Borders::ALL)),
            area,
        );
        return;
    }

    let chrome = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(" octa issues · filter: all · read-only").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        chrome[0],
    );

    let panes = if chrome[1].width >= 72 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(chrome[1])
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(chrome[1])
    };
    draw_list(frame, app, panes[0]);
    draw_detail(frame, app, panes[1]);

    frame.render_widget(
        Paragraph::new(
            " filter: all · ↑/k ↓/j navigate/scroll · Tab focus · PgUp/PgDn · q/Esc quit",
        )
        .style(Style::default().fg(Color::DarkGray)),
        chrome[2],
    );
}

fn focused_block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(format!(" {title} "))
}

fn draw_list(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = focused_block("Issues", app.focus == Focus::List);
    if app.issues.is_empty() {
        frame.render_widget(
            Paragraph::new("No issues yet.")
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    }

    let items = app.issues.iter().map(|detail| {
        let project = detail
            .issue
            .project
            .as_ref()
            .map(|project| project.name.as_str())
            .unwrap_or("No Project");
        let milestone = detail
            .issue
            .milestone
            .as_ref()
            .map(|milestone| milestone.name.as_str())
            .unwrap_or("No Milestone");
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("#{} ", detail.issue.number),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!(
                    "{} · P{} · {} · {} ",
                    detail.issue.state, detail.issue.priority, project, milestone
                ),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(&detail.issue.title),
        ]))
    });
    let list = List::new(items)
        .block(block)
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default()
        .with_selected(Some(app.selected))
        .with_offset(app.list_offset);
    frame.render_stateful_widget(list, area, &mut state);
    app.list_offset = state.offset();
}

fn draw_detail(frame: &mut Frame<'_>, app: &mut App, area: Rect) {
    let block = focused_block("Detail", app.focus == Focus::Detail);
    let Some(detail) = app.selected_issue() else {
        app.max_detail_scroll = 0;
        frame.render_widget(
            Paragraph::new("Select an issue to see its details.")
                .style(Style::default().fg(Color::DarkGray))
                .block(block),
            area,
        );
        return;
    };

    let text = detail_text(detail);
    let inner = block.inner(area);
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    let lines = paragraph.line_count(area.width).min(usize::from(u16::MAX)) as u16;
    let content_lines = lines.saturating_sub(area.height.saturating_sub(inner.height));
    app.max_detail_scroll = content_lines.saturating_sub(inner.height);
    app.detail_scroll = app.detail_scroll.min(app.max_detail_scroll);
    frame.render_widget(paragraph.scroll((app.detail_scroll, 0)), area);
}

fn detail_text(detail: &IssueDetail) -> Text<'static> {
    let issue = &detail.issue;
    let labels = joined(&detail.labels, "(none)");
    let blocked_by = joined_numbers(&detail.blocked_by);
    let blocks = joined_numbers(&detail.blocks);
    let related = joined_numbers(&detail.related);
    let project = issue
        .project
        .as_ref()
        .map(|project| project.name.as_str())
        .unwrap_or("No Project");
    let milestone = issue
        .milestone
        .as_ref()
        .map(|milestone| milestone.name.as_str())
        .unwrap_or("No Milestone");
    let locked_by = issue.locked_by.as_deref().unwrap_or("(unlocked)");
    let parent = detail
        .parent
        .as_ref()
        .map(|parent| format!("#{} {}", parent.number, parent.title))
        .unwrap_or_else(|| "(none)".to_string());
    let sub_issues = if detail.sub_issues.is_empty() {
        "(none)".to_string()
    } else {
        detail
            .sub_issues
            .iter()
            .map(|issue| format!("#{} {}", issue.number, issue.title))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let pull_requests = detail
        .pull_request
        .as_ref()
        .map(|pr| {
            format!(
                "#{} {} · branch: {} · state: {}",
                pr.number, pr.title, pr.branch, pr.state
            )
        })
        .unwrap_or_else(|| "(none)".to_string());
    let mut lines = vec![
        Line::styled(
            format!("#{} {}", issue.number, issue.title),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Line::from(vec![
            Span::styled("State: ", Style::default().fg(Color::DarkGray)),
            Span::raw(issue.state.clone()),
        ]),
        Line::from(format!("Status type: {}", issue.status_type)),
        Line::from(format!("Priority: {}", issue.priority)),
        Line::from(format!("Project: {project}")),
        Line::from(format!("Milestone: {milestone}")),
        Line::from(format!("Locked by: {locked_by}")),
        Line::from(format!("Parent: {parent}")),
        Line::from(format!("Sub-issues: {sub_issues}")),
        Line::from(format!("Labels: {labels}")),
        Line::from(format!("Blocked by: {blocked_by}")),
        Line::from(format!("Blocks: {blocks}")),
        Line::from(format!("Related: {related}")),
        Line::from(format!("Pull requests: {pull_requests}")),
        Line::default(),
        Line::styled("Description", Style::default().fg(Color::Cyan)),
    ];
    if issue.body.is_empty() {
        lines.push(Line::styled(
            "(no description)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        lines.extend(issue.body.lines().map(|line| Line::from(line.to_owned())));
    }
    lines.push(Line::default());
    lines.push(Line::styled("Comments", Style::default().fg(Color::Cyan)));
    if detail.comments.is_empty() {
        lines.push(Line::styled(
            "(no comments)",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        for comment in &detail.comments {
            lines.push(Line::styled(
                format!("— {}", comment.created_at),
                Style::default().fg(Color::DarkGray),
            ));
            lines.extend(comment.body.lines().map(|line| Line::from(line.to_owned())));
        }
    }
    Text::from(lines)
}

fn joined(values: &[String], empty: &str) -> String {
    if values.is_empty() {
        empty.to_string()
    } else {
        values.join(", ")
    }
}

fn joined_numbers(values: &[i64]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values
            .iter()
            .map(|number| format!("#{number}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
pub(crate) fn exercise_view_for_test(issues: Vec<IssueDetail>) {
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(60, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut app = App::new(issues);
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.handle_key(KeyEvent::from(KeyCode::End));
    app.handle_key(KeyEvent::from(KeyCode::Tab));
    app.handle_key(KeyEvent::from(KeyCode::PageDown));
    terminal.draw(|frame| draw(frame, &mut app)).unwrap();
    app.handle_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.quit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        issue::{Issue, IssueRef},
        milestone::MilestoneRef,
        pr::PrRef,
        project::ProjectRef,
        Comment,
    };
    use ratatui::{backend::TestBackend, Terminal};

    fn issue(number: i64, title: &str) -> IssueDetail {
        IssueDetail {
            issue: Issue {
                repo: "octa".into(),
                number,
                title: title.into(),
                body: "A body that can be read.".into(),
                state: "in_progress".into(),
                status_type: "started".into(),
                priority: 2,
                project: Some(ProjectRef {
                    id: 1,
                    name: "CLI launch".into(),
                }),
                milestone: Some(MilestoneRef {
                    id: 2,
                    name: "Public beta".into(),
                }),
                locked_by: Some("agent-a".into()),
                created_at: "2026-07-24".into(),
                updated_at: "2026-07-24".into(),
            },
            labels: vec!["cli".into(), "customer-impact".into()],
            blocks: vec![3],
            blocked_by: vec![1],
            related: vec![4],
            pull_request: Some(PrRef {
                number: 8,
                title: "Ship the TUI".into(),
                branch: "feat/tui".into(),
                state: "open".into(),
            }),
            parent: Some(IssueRef {
                number: 1,
                title: "Parent outcome".into(),
            }),
            sub_issues: vec![IssueRef {
                number: 3,
                title: "Child slice".into(),
            }],
            comments: vec![Comment {
                id: 1,
                body: "A useful comment.".into(),
                created_at: "2026-07-24".into(),
            }],
        }
    }

    fn rendered(app: &mut App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    fn with_body(mut detail: IssueDetail, body: &str) -> IssueDetail {
        detail.issue.body = body.into();
        detail
    }

    #[test]
    fn renders_list_and_complete_selected_detail() {
        let mut app = App::new(vec![issue(2, "Build the TUI")]);
        let screen = rendered(&mut app, 100, 30);
        for expected in [
            "Build the TUI",
            "State: in_progress",
            "Status type: started",
            "Priority: 2",
            "Project: CLI launch",
            "Milestone: Public beta",
            "Locked by: agent-a",
            "Parent: #1 Parent outcome",
            "Sub-issues: #3 Child slice",
            "Labels: cli, customer-impact",
            "Blocked by: #1",
            "Blocks: #3",
            "Related: #4",
            "Pull requests: #8 Ship the TUI",
            "branch: feat/tui",
            "A body that can be read.",
            "A useful comment.",
            "filter: all",
            "read-only",
        ] {
            assert!(screen.contains(expected), "missing {expected:?}:\n{screen}");
        }
    }

    #[test]
    fn list_row_does_not_promote_any_label_to_a_taxonomy_column() {
        let mut app = App::new(vec![issue(2, "Build the TUI")]);
        let backend = TestBackend::new(100, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| draw_list(frame, &mut app, frame.area()))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();

        assert!(screen.contains("CLI launch"), "{screen}");
        assert!(screen.contains("Public beta"), "{screen}");
        assert!(!screen.contains("customer-impact"), "{screen}");
        assert!(!screen.contains("cli"), "{screen}");
    }

    #[test]
    fn default_all_view_keeps_review_terminal_canceled_and_legacy_states_visible() {
        let states = [
            ("In Progress", "started"),
            ("In Review", "started"),
            ("Done", "completed"),
            ("Canceled", "canceled"),
            ("closed", "completed"),
        ];
        let issues = states
            .iter()
            .enumerate()
            .map(|(index, (state, status_type))| {
                let mut detail = issue(index as i64 + 1, &format!("{state} issue"));
                detail.issue.state = (*state).into();
                detail.issue.status_type = (*status_type).into();
                detail
            })
            .collect();
        let mut app = App::new(issues);
        let screen = rendered(&mut app, 120, 30);

        for (index, (state, _)) in states.iter().enumerate() {
            assert!(
                screen.contains(&format!("#{} {state}", index + 1)),
                "missing state {state:?}:\n{screen}"
            );
        }
        assert!(screen.contains("filter: all"), "{screen}");
    }

    #[test]
    fn navigation_focus_and_quit_are_view_only_state_changes() {
        let mut app = App::new(vec![issue(1, "First"), issue(2, "Second")]);
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.selected, 1);
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Detail);
        app.max_detail_scroll = 20;
        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.detail_scroll, 1);
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.quit);
    }

    #[test]
    fn selected_row_and_detail_move_together() {
        let first = with_body(issue(1, "First"), "FIRST DETAIL");
        let second = with_body(issue(2, "Second"), "SECOND DETAIL");
        let mut app = App::new(vec![first, second]);

        app.handle_key(KeyEvent::from(KeyCode::Down));
        let screen = rendered(&mut app, 100, 24);

        assert_eq!(app.selected, 1);
        assert!(screen.contains("SECOND DETAIL"), "{screen}");
        assert!(!screen.contains("FIRST DETAIL"), "{screen}");
    }

    #[test]
    fn list_viewport_follows_selection_to_last_issue() {
        let issues = (1..=20)
            .map(|number| issue(number, &format!("Issue {number:02}")))
            .collect();
        let mut app = App::new(issues);

        app.handle_key(KeyEvent::from(KeyCode::End));
        let screen = rendered(&mut app, 60, 12);

        assert_eq!(app.selected, 19);
        assert!(app.list_offset > 0);
        assert!(screen.contains("Issue 20"), "{screen}");
    }

    #[test]
    fn long_title_does_not_push_list_metadata_offscreen() {
        let mut app = App::new(vec![issue(
            7,
            "A deliberately very long title that is wider than the list pane",
        )]);
        let screen = rendered(&mut app, 60, 14);

        assert!(screen.contains("#7 in_progress · P2"), "{screen}");
    }

    #[test]
    fn wrapped_detail_scroll_reaches_exact_last_line() {
        let body = "alpha beta gamma delta epsilon zeta eta theta\n\
                    a-super-long-unbroken-word-that-also-wraps\n\
                    LAST DETAIL LINE";
        let mut app = App::new(vec![with_body(issue(1, "Wrapped"), body)]);

        // First render calculates the maximum using ratatui's own word
        // wrapper, including whitespace and unbroken-word behavior.
        let _ = rendered(&mut app, 42, 12);
        assert!(app.max_detail_scroll > 0);
        app.handle_key(KeyEvent::from(KeyCode::Tab));
        app.handle_key(KeyEvent::from(KeyCode::End));
        let screen = rendered(&mut app, 42, 12);

        assert_eq!(app.detail_scroll, app.max_detail_scroll);
        assert!(screen.contains("LAST DETAIL LINE"), "{screen}");
    }

    #[test]
    fn empty_and_narrow_states_render_without_panicking() {
        let mut empty = App::new(Vec::new());
        assert!(rendered(&mut empty, 60, 12).contains("No issues yet."));
        assert!(rendered(&mut empty, 19, 4).contains("terminal too"));
    }
}
