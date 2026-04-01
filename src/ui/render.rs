use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::App;

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_chat(f, app, chunks[0]);
    draw_input(f, app, chunks[1]);
    draw_status(f, app, chunks[2]);
}

fn draw_chat(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.chat_log {
        let (prefix, style) = match msg.role.as_str() {
            "user" => ("❯ ", Style::default().fg(t.user).add_modifier(Modifier::BOLD)),
            "assistant" => ("◆ ", Style::default().fg(t.assistant)),
            "system" => ("● ", Style::default().fg(t.system)),
            "error" => ("✗ ", Style::default().fg(t.error)),
            "tool" => ("⚙ ", Style::default().fg(t.tool)),
            _ => ("  ", Style::default().fg(t.fg)),
        };

        let content_lines: Vec<&str> = msg.content.lines().collect();
        if content_lines.is_empty() {
            lines.push(Line::from(Span::styled(prefix.to_string(), style)));
        } else {
            for (i, line) in content_lines.iter().enumerate() {
                let text = if i == 0 {
                    format!("{prefix}{line}")
                } else {
                    format!("  {line}")
                };
                lines.push(Line::from(Span::styled(text, style)));
            }
        }

        if msg.streaming {
            lines.push(Line::from(Span::styled("  ▌", Style::default().fg(t.accent))));
        }

        lines.push(Line::from(""));
    }

    let visible_height = area.height as usize;
    let total_lines = lines.len();
    let scroll = if app.scroll_offset > 0 {
        let max_scroll = total_lines.saturating_sub(visible_height);
        (app.scroll_offset as usize).min(max_scroll) as u16
    } else {
        total_lines.saturating_sub(visible_height) as u16
    };

    let chat = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    f.render_widget(chat, area);
}

fn draw_input(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    let vim_indicator = if app.vim.enabled {
        match app.vim.mode {
            crate::vim::VimMode::Normal => "[N] ",
            crate::vim::VimMode::Insert => "[I] ",
        }
    } else {
        ""
    };

    let voice_indicator = match app.voice.mode {
        crate::voice::VoiceMode::Recording => "🎤 ",
        crate::voice::VoiceMode::Processing => "⏳ ",
        _ => "",
    };

    let prompt = if app.pending_approval.is_some() {
        "[y/n/a] "
    } else if app.waiting_for_response {
        "... "
    } else {
        "❯ "
    };

    let input_text = format!("{vim_indicator}{voice_indicator}{prompt}{}", app.input.buffer);

    let style = if app.pending_approval.is_some() {
        Style::default().fg(t.accent)
    } else {
        Style::default().fg(t.prompt)
    };

    let input = Paragraph::new(input_text)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(t.border)),
        );

    f.render_widget(input, area);

    let prefix_width = UnicodeWidthStr::width(vim_indicator)
        + UnicodeWidthStr::width(voice_indicator)
        + UnicodeWidthStr::width(prompt);
    let cursor_display = app.input.cursor_display_width();
    let cursor_x = area.x + prefix_width as u16 + cursor_display as u16;
    let cursor_y = area.y + 1;
    if cursor_x < area.x + area.width && cursor_y < area.y + area.height {
        f.set_cursor_position((cursor_x, cursor_y));
    }
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;

    let vim_status = if app.vim.enabled {
        match app.vim.mode {
            crate::vim::VimMode::Normal => " VIM:N ",
            crate::vim::VimMode::Insert => " VIM:I ",
        }
    } else {
        ""
    };

    let left = format!(" {} {}", app.status.state, vim_status);
    let right = format!(
        "{}  ↑{}  ↓{}  {} ",
        app.total_usage.format_cost(),
        app.status.tokens_in,
        app.status.tokens_out,
        app.client.model
    );

    let width = area.width as usize;
    let padding = width.saturating_sub(
        UnicodeWidthStr::width(left.as_str()) + UnicodeWidthStr::width(right.as_str()),
    );

    let status_text = format!("{left}{}{right}", " ".repeat(padding));

    let status = Paragraph::new(status_text).style(
        Style::default().bg(t.status_bg).fg(t.status_fg),
    );

    f.render_widget(status, area);
}
