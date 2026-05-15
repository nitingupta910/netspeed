use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, Paragraph,
        Sparkline, Wrap,
    },
    Frame,
};

use crate::app::{App, AppState, HISTORY_SIZE};
use crate::network::{format_bytes, format_speed};

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Length(6),  // speed boxes
            Constraint::Min(8),     // history chart
            Constraint::Length(5),  // speed test panel
            Constraint::Length(3),  // footer / keybindings
        ])
        .split(area);

    render_header(f, app, chunks[0]);
    render_speed_boxes(f, app, chunks[1]);
    render_chart(f, app, chunks[2]);
    render_speedtest_panel(f, app, chunks[3]);
    render_footer(f, chunks[4]);

    if app.interface_selector_open {
        render_interface_popup(f, app, area);
    }
}

// ── Header ──────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let (status_text, status_color) = match app.state {
        AppState::Monitoring => ("● Monitoring", Color::Green),
        AppState::SpeedTesting => ("◎ Testing…", Color::Yellow),
        AppState::SpeedTestDone => ("✓ Test Done", Color::Cyan),
    };

    let line = Line::from(vec![
        Span::styled(" netspeed ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled("  iface: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            &app.interface,
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  │  ", Style::default().fg(Color::DarkGray)),
        Span::styled(status_text, Style::default().fg(status_color)),
        Span::raw(" "),
    ]);

    let widget = Paragraph::new(line)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        )
        .alignment(Alignment::Center);

    f.render_widget(widget, area);
}

// ── Speed boxes ─────────────────────────────────────────────────────────────

fn render_speed_boxes(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_speed_box(
        f,
        "↓  Download",
        app.download_speed,
        app.peak_download,
        app.total_rx,
        &app.download_history,
        Color::Cyan,
        chunks[0],
    );
    render_speed_box(
        f,
        "↑  Upload",
        app.upload_speed,
        app.peak_upload,
        app.total_tx,
        &app.upload_history,
        Color::Green,
        chunks[1],
    );
}

#[allow(clippy::too_many_arguments)]
fn render_speed_box(
    f: &mut Frame,
    title: &str,
    speed: f64,
    peak: f64,
    total: u64,
    history: &std::collections::VecDeque<f64>,
    color: Color,
    area: Rect,
) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // speed value
            Constraint::Length(1), // total / peak
            Constraint::Length(1), // sparkline
            Constraint::Min(0),
        ])
        .split(inner);

    // Speed value
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format_speed(speed),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        rows[0],
    );

    // Total + peak
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("total ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_bytes(total), Style::default().fg(Color::White)),
            Span::styled("  peak ", Style::default().fg(Color::DarkGray)),
            Span::styled(format_speed(peak), Style::default().fg(Color::White)),
        ]))
        .alignment(Alignment::Center),
        rows[1],
    );

    // Sparkline (last HISTORY_SIZE samples scaled to Kbps as u64)
    let spark_data: Vec<u64> = history.iter().map(|&v| (v * 1_000.0) as u64).collect();
    if !spark_data.is_empty() {
        let sparkline = Sparkline::default()
            .data(&spark_data)
            .style(Style::default().fg(color).bg(Color::Reset));
        f.render_widget(sparkline, rows[2]);
    }
}

// ── History chart ────────────────────────────────────────────────────────────

fn render_chart(f: &mut Frame, app: &App, area: Rect) {
    let dl: Vec<(f64, f64)> = app.download_history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let ul: Vec<(f64, f64)> = app.upload_history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let max_y = app.download_history.iter()
        .chain(app.upload_history.iter())
        .cloned()
        .fold(1.0f64, f64::max)
        * 1.25;

    let mid_label = format_speed(max_y / 2.0);
    let max_label = format_speed(max_y);

    let datasets = vec![
        Dataset::default()
            .name("↓ Download")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&dl),
        Dataset::default()
            .name("↑ Upload")
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Green))
            .data(&ul),
    ];

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::styled(
                    " Speed History (60 s) ",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, HISTORY_SIZE as f64])
                .labels(vec![
                    Span::styled("60s ago", Style::default().fg(Color::DarkGray)),
                    Span::styled("30s ago", Style::default().fg(Color::DarkGray)),
                    Span::styled("now", Style::default().fg(Color::DarkGray)),
                ]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_y])
                .labels(vec![
                    Span::styled("0", Style::default().fg(Color::DarkGray)),
                    Span::styled(mid_label, Style::default().fg(Color::DarkGray)),
                    Span::styled(max_label, Style::default().fg(Color::DarkGray)),
                ]),
        );

    f.render_widget(chart, area);
}

// ── Speed test panel ─────────────────────────────────────────────────────────

fn render_speedtest_panel(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled(
            " Speed Test ",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    match &app.state {
        AppState::Monitoring => {
            if let Some(error) = app
                .speed_test_progress
                .as_deref()
                .filter(|text| text.starts_with("Error:"))
            {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        error,
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD),
                    )))
                    .wrap(Wrap { trim: true })
                    .alignment(Alignment::Center),
                    rows[0],
                );
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "Press [s] to try again",
                        Style::default().fg(Color::DarkGray),
                    )))
                    .alignment(Alignment::Center),
                    rows[1],
                );
            } else if let Some(result) = &app.speed_test_result {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("↓ ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            format_speed(result.download_mbps),
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("     "),
                        Span::styled("↑ ", Style::default().fg(Color::Green)),
                        Span::styled(
                            format_speed(result.upload_mbps),
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                        ),
                    ]))
                    .alignment(Alignment::Center),
                    rows[0],
                );
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "Press [s] to run again",
                        Style::default().fg(Color::DarkGray),
                    )))
                    .alignment(Alignment::Center),
                    rows[1],
                );
            } else {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "Press [s] to measure your line speed",
                        Style::default().fg(Color::DarkGray),
                    )))
                    .alignment(Alignment::Center),
                    rows[0],
                );
            }
        }

        AppState::SpeedTesting => {
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    app.speed_test_progress.as_deref().unwrap_or("Running…"),
                    Style::default().fg(Color::Yellow),
                )))
                .alignment(Alignment::Center),
                rows[0],
            );
        }

        AppState::SpeedTestDone => {
            if let Some(result) = &app.speed_test_result {
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("↓ ", Style::default().fg(Color::Cyan)),
                        Span::styled(
                            format_speed(result.download_mbps),
                            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("     "),
                        Span::styled("↑ ", Style::default().fg(Color::Green)),
                        Span::styled(
                            format_speed(result.upload_mbps),
                            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                        ),
                    ]))
                    .alignment(Alignment::Center),
                    rows[0],
                );
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "Press [s] to run again",
                        Style::default().fg(Color::DarkGray),
                    )))
                    .alignment(Alignment::Center),
                    rows[1],
                );
            }
        }
    }
}

// ── Footer ───────────────────────────────────────────────────────────────────

fn render_footer(f: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        key("[q]"), Span::raw(" Quit  "),
        key("[s]"), Span::raw(" Speed Test  "),
        key("[i]"), Span::raw(" Interface  "),
        key("[↑↓ Enter]"), Span::raw(" Navigate popup  "),
        key("[Esc]"), Span::raw(" Close"),
    ]);

    f.render_widget(
        Paragraph::new(line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            )
            .alignment(Alignment::Center),
        area,
    );
}

fn key(s: &str) -> Span<'static> {
    Span::styled(s.to_owned(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
}

// ── Interface selector popup ─────────────────────────────────────────────────

fn render_interface_popup(f: &mut Frame, app: &App, area: Rect) {
    let width = 42u16.min(area.width.saturating_sub(4));
    let height = (app.available_interfaces.len() as u16 + 2).min(area.height.saturating_sub(4));

    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup);

    let items: Vec<ListItem> = app.available_interfaces
        .iter()
        .enumerate()
        .map(|(i, iface)| {
            let selected = i == app.selected_interface_idx;
            let prefix = if selected { "▶ " } else { "  " };
            let style = if selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(format!("{prefix}{iface}"), style)))
        })
        .collect();

    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(Span::styled(
                    " Select Interface ",
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        popup,
    );
}
