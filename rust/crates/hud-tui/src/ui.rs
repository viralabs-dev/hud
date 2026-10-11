use crate::application::App;
use hud_domain::{DropTarget, PanelRect, Rect as CellRect};
use ratatui::{
    layout::{Constraint, Direction, Layout as Grid, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
    Frame,
};

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.rects.clear();
    app.tab_hits.clear();
    if area.width < 30 || area.height < 12 {
        app.cancel_drag();
        frame.render_widget(
            Paragraph::new("Terminal too small: resize to at least 30×12; Ctrl+C exits"),
            area,
        );
        return;
    }
    let rows = Grid::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);
    let body = rows[0];
    if app.emphasis {
        panel(frame, app, app.focus.clone(), body);
    } else if let Some(custom) = &app.custom {
        let mut auto: std::collections::BTreeMap<String, u16> = app
            .custom_content
            .iter()
            .map(|(id, content)| {
                (
                    id.clone(),
                    content
                        .lines()
                        .count()
                        .saturating_add(2)
                        .min(u16::MAX as usize) as u16,
                )
            })
            .collect();
        let commands_height = app
            .commands
            .as_ref()
            .map_or(2, |commands| commands.names().len().saturating_add(2));
        auto.insert(
            "commands".into(),
            commands_height.min(u16::MAX as usize) as u16,
        );
        auto.insert(
            "usage_codex".into(),
            app.resource_views.get("usage_codex").map_or(3, |view| {
                view.lines()
                    .count()
                    .saturating_add(2)
                    .min(u16::MAX as usize) as u16
            }),
        );
        let geometry = if app.python_default_geometry {
            crate::default_layout::geometry_custom(custom, body.width)
        } else {
            custom.clone()
        };
        match geometry.geometry(body.width, body.height, &auto) {
            Ok(rects) => {
                for entry in rects {
                    let area = Rect::new(
                        body.x + entry.rect.col,
                        body.y + entry.rect.row,
                        entry.rect.width,
                        entry.rect.height,
                    );
                    panel(frame, app, entry.id, area);
                }
            }
            Err(error) => {
                app.message = format!("Custom geometry refused: {error}");
            }
        }
    } else {
        let columns = app.layout.columns().to_vec();
        let sizes: Vec<_> = columns
            .iter()
            .map(|_| Constraint::Ratio(1, columns.len() as u32))
            .collect();
        let cols = Grid::default()
            .direction(Direction::Horizontal)
            .constraints(sizes)
            .split(body);
        for (index, ids) in columns.iter().enumerate() {
            let sizes: Vec<_> = ids
                .iter()
                .map(|_| Constraint::Ratio(1, ids.len() as u32))
                .collect();
            let cells = Grid::default()
                .direction(Direction::Vertical)
                .constraints(sizes)
                .split(cols[index]);
            for (slot, id) in ids.iter().enumerate() {
                panel(frame, app, id.clone(), cells[slot]);
            }
        }
    }
    let input_view = app.input.as_ref().map(|input| {
        crate::interaction::input_view(
            input,
            app.editor.cursor,
            usize::from(rows[1].width.saturating_sub(6)),
        )
    });
    let mode_color = match app.view_tab {
        crate::interaction::ViewTab::Claude => Color::Indexed(208),
        crate::interaction::ViewTab::Codex => Color::Indexed(248),
        crate::interaction::ViewTab::OpenCode => Color::Indexed(141),
        _ => Color::Cyan,
    };
    let title = match app.view_tab {
        crate::interaction::ViewTab::Notes => "─ ENTRADA ".into(),
        tab => format!(
            "─ {} · {} · {} ",
            match tab {
                crate::interaction::ViewTab::Claude => "CLAUDE",
                crate::interaction::ViewTab::Codex => "CODEX",
                _ => "OPENCODE",
            },
            app.agents.as_ref().map_or("LEITURA", |agents| {
                if agents.profile() == hud_agent_protocol::Profile::Full {
                    "COMPLETO"
                } else {
                    "LEITURA"
                }
            }),
            app.agents
                .as_ref()
                .map_or("DESATIVADO", |agents| agents.mode_label())
        ),
    };
    let text = input_view.as_ref().map_or_else(
        || {
            if app.view_tab.provider().is_some() {
                "pergunte ao agente · / para comandos · Alt+1 notas".into()
            } else {
                "escreva uma nota · / para comandos · /ajuda mostra tudo".into()
            }
        },
        |(text, _)| text.clone(),
    );
    let line = ratatui::text::Line::from(vec![
        ratatui::text::Span::styled(
            if app.view_tab.provider().is_some() {
                "✦ "
            } else {
                "› "
            },
            Style::default().fg(mode_color),
        ),
        ratatui::text::Span::styled(
            text,
            Style::default().fg(if app.input.is_some() {
                Color::White
            } else {
                Color::DarkGray
            }),
        ),
    ]);
    let hint = if app.message.starts_with("Rust preview") {
        " texto = nota · Alt+2/3/4 agentes · /ajuda ".into()
    } else {
        format!(" {} ", hud_resources::sanitize(&app.message))
    };
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .padding(Padding::horizontal(1))
                .border_style(Style::default().fg(if app.view_tab.provider().is_some() {
                    mode_color
                } else {
                    Color::Blue
                }))
                .title_style(Style::default().fg(mode_color).add_modifier(Modifier::BOLD))
                .title(title)
                .title_bottom(ratatui::text::Line::styled(
                    hint,
                    Style::default().fg(Color::DarkGray),
                )),
        ),
        rows[1],
    );
    if app.view_tab.provider().is_some() {
        if let Some(review) = app
            .agents
            .as_ref()
            .and_then(|agents| agents.confirmation_review())
        {
            let text = crate::markdown::render(
                &review.join("\n\n"),
                usize::from(body.width.saturating_sub(2)),
            );
            let index = app.view_tab.index();
            let max_scroll = text
                .lines
                .len()
                .saturating_sub(usize::from(body.height.saturating_sub(2)));
            app.scroll_back[index] = app.scroll_back[index].min(max_scroll);
            let scroll = max_scroll
                .saturating_sub(app.scroll_back[index])
                .min(u16::MAX as usize) as u16;
            frame.render_widget(Clear, body);
            frame.render_widget(
                Paragraph::new(text).scroll((scroll, 0)).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" REAL · envia contexto · quota possível · yes/cancel ")
                        .title_bottom(
                            " PgUp/PgDn/roda: revisar detalhes; nenhum processo iniciado ",
                        ),
                ),
                body,
            );
        }
    }
    if let Some((_, cursor)) = input_view {
        frame.set_cursor_position((
            rows[1]
                .x
                .saturating_add(4)
                .saturating_add(cursor as u16)
                .min(rows[1].right().saturating_sub(3)),
            rows[1].y + 1,
        ));
    }
}

fn panel(frame: &mut Frame, app: &mut App, id: hud_domain::PanelId, area: Rect) {
    if let Ok(rect) = CellRect::new(area.y, area.x, area.height, area.width) {
        app.rects.push(PanelRect {
            id: id.clone(),
            rect,
        });
    }
    let drop_here = match &app.target {
        Some(DropTarget::Swap(target)) => target == &id,
        Some(DropTarget::Move { column, index: _ }) => app
            .layout
            .columns()
            .get(*column)
            .map(|ids| ids.contains(&id))
            .unwrap_or(false),
        None => false,
    };
    let color = if drop_here {
        Color::Green
    } else if app.focus == id {
        Color::Cyan
    } else {
        Color::Blue
    };
    let title = if drop_here {
        format!("{} · DROP HERE", id.as_str())
    } else {
        app.custom
            .as_ref()
            .and_then(|custom| custom.panels.get(id.as_str()))
            .map_or_else(
                || {
                    app.custom
                        .as_ref()
                        .and_then(|custom| custom.blocks.get(id.as_str()))
                        .map_or_else(
                            || id.as_str().to_string(),
                            |block| {
                                if block.title.is_empty() {
                                    crate::application::builtin_id(id.as_str()).to_string()
                                } else {
                                    block.title.clone()
                                }
                            },
                        )
                },
                |panel| panel.title.clone(),
            )
    };
    let output_panel = crate::application::builtin_id(id.as_str()) == "output";
    if crate::application::builtin_id(id.as_str()) == "agenda" {
        app.agenda_capacity
            .set(usize::from(area.height.saturating_sub(2)));
    }
    let content = hud_resources::sanitize(&if output_panel && app.view_tab.provider().is_some() {
        app.agent_display()
    } else {
        app.panel_content(id.as_str())
    });
    let color = if output_panel && !drop_here && app.focus != id {
        match app.view_tab {
            crate::interaction::ViewTab::Claude => Color::Indexed(208),
            crate::interaction::ViewTab::Codex => Color::Indexed(248),
            crate::interaction::ViewTab::OpenCode => Color::Indexed(141),
            _ => color,
        }
    } else {
        color
    };
    let title = if output_panel {
        let prefix = if app.view_tab.provider().is_some() {
            format!(
                "─ {title} · {} ",
                app.agents
                    .as_ref()
                    .map_or("DESATIVADO", |agents| agents.mode_label())
            )
        } else {
            format!("─ {title} ")
        };
        let mut spans = vec![ratatui::text::Span::raw(prefix)];
        let mut column = area.x + 1 + ratatui::text::Line::from(spans.clone()).width() as u16;
        for (digit, label, tab, tab_color) in [
            (
                '1',
                " 1 NOTAS ",
                crate::interaction::ViewTab::Notes,
                Color::Cyan,
            ),
            (
                '2',
                " 2 CLAUDE ",
                crate::interaction::ViewTab::Claude,
                Color::Indexed(208),
            ),
            (
                '3',
                " 3 CODEX ",
                crate::interaction::ViewTab::Codex,
                Color::Indexed(248),
            ),
            (
                '4',
                " 4 OPENCODE ",
                crate::interaction::ViewTab::OpenCode,
                Color::Indexed(141),
            ),
        ] {
            let width = label.len() as u16;
            if column + width > area.right().saturating_sub(1) {
                break;
            }
            let style = Style::default().fg(tab_color);
            let style = if app.view_tab == tab {
                style.add_modifier(
                    ratatui::style::Modifier::REVERSED | ratatui::style::Modifier::BOLD,
                )
            } else {
                style.add_modifier(Modifier::DIM)
            };
            spans.push(ratatui::text::Span::styled(label, style));
            if let Ok(rect) = CellRect::new(area.y, column, 1, width) {
                app.tab_hits.push((rect, digit));
            }
            column += width;
        }
        ratatui::text::Line::from(spans)
    } else {
        ratatui::text::Line::from(format!("─ {title} "))
    };
    let styled = if crate::application::builtin_id(id.as_str()) == "output" {
        crate::markdown::render(&content, usize::from(area.width.saturating_sub(4)))
    } else if crate::application::builtin_id(id.as_str()) == "system"
        && app.metrics.is_some()
        && !app.custom_content.contains_key(id.as_str())
    {
        system_lines(
            app,
            usize::from(area.width.saturating_sub(4)),
            usize::from(area.height.saturating_sub(2)),
        )
    } else if crate::application::builtin_id(id.as_str()) == "vault" {
        ratatui::text::Text::from(
            content
                .lines()
                .map(|line| {
                    ratatui::text::Line::styled(
                        line.to_owned(),
                        if matches!(line, "RECENTES" | "QUADROS" | "ATENÇÃO") {
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        },
                    )
                })
                .collect::<Vec<_>>(),
        )
    } else {
        ratatui::text::Text::raw(content)
    };
    let scroll = if output_panel {
        styled
            .lines
            .len()
            .saturating_sub(usize::from(area.height.saturating_sub(2)))
            .saturating_sub(app.scroll_back[app.view_tab.index()])
            .min(u16::MAX as usize) as u16
    } else {
        0
    };
    let mut block =
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .padding(Padding::horizontal(1))
            .title(title)
            .title_style(
                Style::default()
                    .fg(if output_panel { color } else { Color::Cyan })
                    .add_modifier(Modifier::BOLD),
            )
            .border_style(Style::default().fg(color).add_modifier(
                if drop_here || app.focus == id {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                },
            ));
    if crate::application::builtin_id(id.as_str()) == "system"
        && !app.custom_content.contains_key(id.as_str())
    {
        let mut uptime_width = 0;
        if let Some(uptime) = app
            .metrics
            .as_ref()
            .and_then(|m| m.uptime_seconds)
            .filter(|v| v.is_finite() && *v >= 0.)
        {
            let hours = (uptime / 3600.) as u64;
            let text = if hours >= 24 {
                format!(" ligado há {}d {}h ", hours / 24, hours % 24)
            } else {
                format!(" ligado há {}h {}m ", hours, (uptime / 60.) as u64 % 60)
            };
            uptime_width = ratatui::text::Line::from(text.as_str()).width();
            block = block.title_bottom(ratatui::text::Line::styled(
                text,
                Style::default().fg(Color::DarkGray),
            ));
        }
        if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            if let Ok(candidates) = hud_data::clock_candidates(now.as_secs() as i64) {
                let time = choose_clock(&candidates, usize::from(area.width), uptime_width);
                block = block.title_bottom(
                    ratatui::text::Line::styled(
                        time,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    )
                    .alignment(ratatui::layout::Alignment::Right),
                );
            }
        }
    }
    frame.render_widget(
        Paragraph::new(styled)
            .style(
                Style::default().fg(
                    if crate::application::builtin_id(id.as_str()).starts_with("usage_")
                        && !app
                            .resource_views
                            .contains_key(crate::application::builtin_id(id.as_str()))
                    {
                        Color::DarkGray
                    } else {
                        Color::White
                    },
                ),
            )
            .scroll((scroll, 0))
            .block(block),
        area,
    );
}

fn clip_cells(text: &str, width: usize) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        let mut next = out.clone();
        next.push(ch);
        if ratatui::text::Line::from(next.as_str()).width() > width {
            break;
        }
        out = next;
    }
    out
}
fn choose_clock(candidates: &[String], width: usize, uptime_width: usize) -> String {
    candidates
        .iter()
        .find(|text| ratatui::text::Line::from(text.as_str()).width() + uptime_width + 4 <= width)
        .or_else(|| candidates.last())
        .map_or_else(String::new, |text| {
            clip_cells(text, width.saturating_sub(uptime_width + 4))
        })
}
fn history_line(history: &[Option<f64>]) -> ratatui::text::Line<'static> {
    use ratatui::text::{Line, Span};
    let mut spans = vec![Span::raw("      ")];
    for sample in history {
        let value = sample.filter(|v| v.is_finite()).map(|v| v.clamp(0., 100.));
        let symbol = value.map_or('·', |v| {
            ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][((v / 100. * 8.) as usize).min(7)]
        });
        let color = match value {
            Some(v) if v >= 90. => Color::Red,
            Some(v) if v >= 70. => Color::Yellow,
            Some(_) => Color::Green,
            None => Color::DarkGray,
        };
        spans.push(Span::styled(symbol.to_string(), Style::default().fg(color)));
    }
    Line::from(spans)
}
fn system_lines(app: &App, width: usize, room: usize) -> ratatui::text::Text<'static> {
    use ratatui::text::{Line, Span, Text};
    let Some(sample) = &app.metrics else {
        return Text::raw("Aguardando métricas…");
    };
    let dim = Style::default().fg(Color::DarkGray);
    fn amount(value: u64) -> String {
        if value >= 1024 * 1024 * 1024 {
            format!("{:.1}G", value as f64 / (1024. * 1024. * 1024.))
        } else {
            format!("{:.0}M", value as f64 / (1024. * 1024.))
        }
    }
    fn meter(
        label: &str,
        pct: Option<f64>,
        tail: String,
        width: usize,
        tail_width: usize,
    ) -> Line<'static> {
        let label = clip_cells(label, 5);
        let label = format!(
            "{}{}",
            label,
            " ".repeat(6 - ratatui::text::Line::from(label.clone()).width())
        );
        let tail = clip_cells(&tail, tail_width);
        let bar_width = width.saturating_sub(12 + tail_width).max(4);
        let pct = pct.filter(|v| v.is_finite()).map(|v| v.clamp(0., 100.));
        let filled = pct.map_or(0, |v| {
            ((bar_width as f64 * v / 100.).round() as usize).min(bar_width)
        });
        let color = match pct {
            Some(v) if v >= 90. => Color::Red,
            Some(v) if v >= 70. => Color::Yellow,
            Some(_) => Color::Green,
            None => Color::DarkGray,
        };
        Line::from(vec![
            Span::styled(label, Style::default().fg(Color::DarkGray)),
            Span::styled("█".repeat(filled), Style::default().fg(color)),
            Span::styled(
                "░".repeat(bar_width - filled),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                pct.map_or_else(|| "   — ".into(), |v| format!(" {v:3.0}%")),
                Style::default().fg(color),
            ),
            Span::styled(format!(" {tail}"), Style::default().fg(Color::DarkGray)),
        ])
    }
    let mut tails = vec![sample
        .cores
        .map_or_else(|| "—".into(), |v| format!("{v} núcleos"))];
    if let Some(m) = &sample.memory {
        tails.push(format!("{}/{}", amount(m.used), amount(m.total)));
        if m.swap_total > 0 {
            tails.push(format!("{}/{}", amount(m.swap_used), amount(m.swap_total)));
        }
    }
    tails.extend(
        sample
            .disks
            .iter()
            .take(2)
            .map(|d| format!("{} livre", amount(d.total.saturating_sub(d.used)))),
    );
    let tail_width = tails
        .iter()
        .map(|t| ratatui::text::Line::from(t.as_str()).width())
        .max()
        .unwrap_or(0)
        .min(width.saturating_sub(19));
    let mut lines = Vec::new();
    let mut ranks = Vec::new();
    if app.shows("system", "cpu") {
        ranks.push(1);
        lines.push(meter(
            "CPU",
            sample.cpu_percent,
            sample
                .cores
                .map_or_else(|| "—".into(), |v| format!("{v} núcleos")),
            width,
            tail_width,
        ));
    }
    if app.shows("system", "historico") {
        let history = app.cpu_history(width.saturating_sub(6));
        if !history.is_empty() {
            ranks.push(4);
            lines.push(history_line(&history));
        }
    }
    if app.shows("system", "mem") {
        ranks.push(1);
        lines.push(meter(
            "MEM",
            sample
                .memory
                .as_ref()
                .and_then(|v| (v.total > 0).then(|| 100. * v.used as f64 / v.total as f64)),
            sample.memory.as_ref().map_or_else(
                || "—".into(),
                |v| format!("{}/{}", amount(v.used), amount(v.total)),
            ),
            width,
            tail_width,
        ));
    }
    if app.shows("system", "swap") {
        if let Some(m) = &sample.memory {
            if m.swap_total > 0 {
                ranks.push(3);
                lines.push(meter(
                    "SWAP",
                    Some(100. * m.swap_used as f64 / m.swap_total as f64),
                    format!("{}/{}", amount(m.swap_used), amount(m.swap_total)),
                    width,
                    tail_width,
                ));
            }
        }
    }
    if app.shows("system", "disco") {
        for (index, disk) in sample.disks.iter().take(2).enumerate() {
            ranks.push(if index == 0 { 2 } else { 7 });
            lines.push(meter(
                &hud_resources::short(&disk.label, 5),
                (disk.total > 0).then(|| 100. * disk.used as f64 / disk.total as f64),
                format!("{} livre", amount(disk.total.saturating_sub(disk.used))),
                width,
                tail_width,
            ));
        }
    }
    if app.shows("system", "load") {
        ranks.push(2);
        let processes = sample
            .processes
            .map_or_else(String::new, |v| format!("{v} proc"));
        let tail = clip_cells(&processes, width.saturating_sub(8));
        let load = sample.load.map_or_else(
            || "—".into(),
            |v| {
                let trend = if v[0] > v[1] * 1.1 {
                    "↑"
                } else if v[0] < v[1] * 0.9 {
                    "↓"
                } else {
                    "→"
                };
                format!("{:.2} {trend} {:.2}  {:.2}", v[0], v[1], v[2])
            },
        );
        let load = clip_cells(
            &load,
            width.saturating_sub(7 + ratatui::text::Line::from(tail.as_str()).width()),
        );
        let gap = width.saturating_sub(
            6 + ratatui::text::Line::from(load.as_str()).width()
                + ratatui::text::Line::from(tail.as_str()).width(),
        );
        lines.push(Line::from(vec![
            Span::styled("LOAD  ", dim),
            Span::raw(load),
            Span::raw(" ".repeat(gap)),
            Span::styled(tail, dim),
        ]));
    }
    if app.shows("system", "rede") {
        ranks.push(2);
        lines.push(Line::from(vec![
            Span::styled("REDE  ", dim),
            Span::styled(
                sample.network_bytes_per_second.map_or_else(
                    || "—".into(),
                    |(r, t)| format!("↓ {:.1}K/s  ↑ {:.1}K/s", r / 1024., t / 1024.),
                ),
                Style::default().fg(Color::Cyan),
            ),
        ]));
    }
    if app.shows("system", "sensores") {
        ranks.push(3);
        let temp = sample
            .temperature_celsius
            .map_or_else(|| "—".into(), |v| format!("{v:.0}°C"));
        let battery = sample
            .battery
            .as_ref()
            .map_or_else(String::new, |(v, status)| {
                format!(" · bateria {v}% {}", hud_resources::short(status, 20))
            });
        lines.push(Line::from(vec![
            Span::styled("TEMP  ", dim),
            Span::raw(format!("{temp}{battery}")),
        ]));
    }
    let mut priority: Vec<(usize, usize)> = ranks
        .into_iter()
        .enumerate()
        .map(|(index, rank)| (rank, index))
        .collect();
    priority.sort_unstable();
    let mut keep: Vec<usize> = priority
        .into_iter()
        .take(room)
        .map(|(_, index)| index)
        .collect();
    keep.sort_unstable();
    Text::from(
        lines
            .into_iter()
            .enumerate()
            .filter_map(|(index, line)| keep.binary_search(&index).is_ok().then_some(line))
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};
    #[test]
    fn unicode_cells_and_target_style_survive_render_and_resize() {
        let mut app = App::new();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let buffer = terminal.backend().buffer();
        let symbols: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(symbols.contains("ação"));
        assert!(symbols.contains("日"));
        assert!(symbols.contains("本"));
        assert!(symbols.contains("語"));
        let found = buffer
            .content()
            .iter()
            .position(|c| c.symbol() == "日")
            .unwrap();
        assert_eq!(
            buffer.content()[found + 1].symbol(),
            " ",
            "wide glyph occupies two cells"
        );
        app.target = Some(DropTarget::Swap(hud_domain::PanelId::new("vault").unwrap()));
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let symbols: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(symbols.contains("DROP HERE"));
        assert!(terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|c| c.fg == Color::Green));
        app.focus = hud_domain::PanelId::new("vault").unwrap();
        app.toggle_emphasis();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.rects.len(), 1);
        assert_eq!(app.rects[0].id.as_str(), "vault");
        app.cancel();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.rects.len(), 4);
    }
    #[test]
    fn render_builds_hit_regions_and_small_resize_clears_them() {
        let mut app = App::new();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.rects.len(), 4);
        let mut small = Terminal::new(TestBackend::new(20, 8)).unwrap();
        small.draw(|frame| render(frame, &mut app)).unwrap();
        assert!(app.rects.is_empty());
    }
    #[test]
    fn custom_geometry_preserves_dimensions_and_refuses_drag() {
        let mut app = App::new();
        let custom = hud_custom::Custom::parse(br#"{"version":1,"name":"demo","columns":[{"width":35,"slots":[{"id":"system","height":7},{"id":"note","weight":1}]},{"slots":[{"id":"vault","weight":1}]}],"panels":[{"id":"note","title":"Note","type":"text","text":"hello"}]}"#).unwrap();
        app.layout = custom.layout().unwrap();
        app.custom = Some(custom);
        app.custom_content
            .insert("note".into(), "ação 日本語".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.rects[0].rect.width, 35);
        assert_eq!(app.rects[0].rect.height, 7);
        assert_eq!(app.rects[2].rect.col, 35);
        let original = app.layout.columns().to_vec();
        app.press(hud_domain::Cell { row: 0, col: 3 });
        app.motion(hud_domain::Cell { row: 8, col: 40 });
        app.release(hud_domain::Cell { row: 8, col: 40 });
        assert_eq!(app.layout.columns(), original);
        assert!(app.target.is_none());
        let symbols: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(symbols.contains("ação"));
        app.toggle_emphasis();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        assert_eq!(app.rects.len(), 1);
    }
    #[test]
    fn default_dashboard_tabs_preserve_panels_and_input_and_allow_title_drag() {
        for (width, height) in [(80, 24), (120, 40), (160, 50)] {
            let mut app = App::python_default().unwrap();
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            app.begin_text_input();
            app.input_insert('猫');
            for digit in ['2', '3', '4', '1'] {
                app.select_tab(digit);
                terminal.draw(|frame| render(frame, &mut app)).unwrap();
                assert_eq!(app.rects.len(), 7);
                assert_eq!(app.input.as_deref(), Some("猫"));
            }
            if width >= 120 {
                let (rect, _) = app
                    .tab_hits
                    .iter()
                    .find(|(_, digit)| *digit == '2')
                    .unwrap();
                app.press(hud_domain::Cell {
                    row: rect.row,
                    col: rect.col,
                });
                assert!(app.view_tab.provider().is_some());
                assert_eq!(app.input.as_deref(), Some("猫"));
            }
            app.press(hud_domain::Cell { row: 0, col: 3 });
            assert!(
                app.message.starts_with("Dragging title"),
                "default title can start an in-memory drag"
            );
            let target = app
                .rects
                .iter()
                .find(|p| p.id.as_str() == "vault")
                .unwrap()
                .rect;
            let destination = hud_domain::Cell {
                row: target.row + 2,
                col: target.col + 3,
            };
            app.motion(destination);
            app.release(destination);
            assert!(
                app.message.starts_with("Layout changed in memory")
                    || app.message.starts_with("Layout drop refused")
            );
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert_eq!(
                app.rects.len(),
                7,
                "a drop must preserve a visible dashboard"
            );
            let origin = app
                .rects
                .iter()
                .find(|p| crate::application::builtin_id(p.id.as_str()) == "system")
                .unwrap()
                .rect;
            app.press(hud_domain::Cell {
                row: origin.row,
                col: origin.col + 3,
            });
            assert!(
                app.message.starts_with("Dragging title"),
                "a second drag remains available"
            );
        }
    }
    #[test]
    fn python_reference_padding_hierarchy_and_unicode_composer_at_three_sizes() {
        for (width, height) in [(80, 24), (120, 40), (160, 50)] {
            let mut app = App::python_default().unwrap();
            app.metrics = Some(hud_metrics::Sample {
                cpu_percent: Some(25.),
                cores: Some(8),
                memory: Some(hud_metrics::Memory {
                    used: 1024 * 1024 * 1024,
                    total: 4 * 1024 * 1024 * 1024,
                    swap_used: 0,
                    swap_total: 0,
                }),
                ..Default::default()
            });
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            let system = app
                .rects
                .iter()
                .find(|p| p.id.as_str() == "sistema")
                .unwrap()
                .rect;
            let vault = app
                .rects
                .iter()
                .find(|p| p.id.as_str() == "vault")
                .unwrap()
                .rect;
            let buffer = terminal.backend().buffer();
            let cell = |x: u16, y: u16| {
                &buffer.content()[usize::from(y) * usize::from(width) + usize::from(x)]
            };
            assert_eq!(cell(system.col + 3, system.row).symbol(), "S");
            assert_eq!(cell(system.col + 2, system.row + 1).symbol(), "C");
            assert_eq!(cell(system.col + 1, system.row + 1).symbol(), " ");
            assert_eq!(cell(vault.col, vault.row + 1).fg, Color::Blue);
            assert_eq!(cell(vault.col + 3, vault.row).fg, Color::Cyan);
            assert!(cell(vault.col + 3, vault.row)
                .modifier
                .contains(Modifier::BOLD));
            assert_eq!(cell(2, height - 2).symbol(), "›");
            assert_eq!(cell(4, height - 2).symbol(), "e");
            assert_eq!(cell(4, height - 2).fg, Color::DarkGray);
            app.begin_text_input();
            for c in "café 日本語 🐱".repeat(30).chars() {
                app.input_insert(c);
            }
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert_eq!(app.rects.len(), 7);
            let pos = terminal.get_cursor_position().unwrap();
            assert!(pos.x >= 4 && pos.x < width - 2);
            assert_eq!(pos.y, height - 2);
            terminal.backend_mut().resize(29, 11);
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert!(app.rects.is_empty());
            assert!(app.tab_hits.is_empty());
            terminal.backend_mut().resize(width, height);
            terminal.draw(|frame| render(frame, &mut app)).unwrap();
            assert_eq!(app.rects.len(), 7);
        }
    }
    #[test]
    fn system_meters_align_percentages_and_bound_unicode_labels_without_debug_values() {
        let mut app = App::python_default().unwrap();
        app.metrics = Some(hud_metrics::Sample {
            cpu_percent: None,
            cores: Some(8),
            memory: Some(hud_metrics::Memory {
                used: 1024 * 1024 * 1024,
                total: 4 * 1024 * 1024 * 1024,
                swap_used: 1,
                swap_total: 1024,
            }),
            disks: vec![hud_metrics::Disk {
                label: "日本語disk".into(),
                used: 20,
                total: 100,
            }],
            ..Default::default()
        });
        let lines = system_lines(&app, 34, 30);
        let strings: Vec<String> = lines
            .lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(lines.lines.iter().take(4).all(|l| l.width() <= 34));
        assert!(strings[0].contains('—'));
        assert!(!strings.join("\n").contains("Some("));
        let columns: Vec<_> = strings
            .iter()
            .skip(1)
            .take(3)
            .map(|s| ratatui::text::Line::from(s.split('%').next().unwrap()).width())
            .collect();
        assert!(columns.windows(2).all(|pair| pair[0] == pair[1]));
        app.metrics.as_mut().unwrap().disks.push(hud_metrics::Disk {
            label: "MEMORY".into(),
            used: 10,
            total: 100,
        });
        for width in [34, 54, 74] {
            let compact = system_lines(&app, width, 5);
            let text = compact
                .lines
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            for label in ["CPU", "MEM", "LOAD", "REDE", "日本"] {
                assert!(text.contains(label), "{text}");
            }
            assert!(!text.contains("SWAP"));
            assert!(!text.contains("MEMOR"));
        }
    }
    #[test]
    fn visual_details_preserve_levels_clock_fit_and_load_tail() {
        let history = history_line(&[None, Some(0.), Some(75.), Some(95.), Some(f64::NAN)]);
        assert_eq!(history.spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(history.spans[2].style.fg, Some(Color::Green));
        assert_eq!(history.spans[3].style.fg, Some(Color::Yellow));
        assert_eq!(history.spans[4].style.fg, Some(Color::Red));
        let clock = vec![
            " dom 11/10  12:34:56 ".into(),
            " 11/10 12:34:56 ".into(),
            " 12:34:56 ".into(),
            " 12:34 ".into(),
        ];
        assert_eq!(choose_clock(&clock, 80, 20), clock[0]);
        assert_eq!(choose_clock(&clock, 34, 20), clock[2]);
        assert_eq!(choose_clock(&clock, 31, 20), clock[3]);
        let mut app = App::python_default().unwrap();
        app.metrics = Some(hud_metrics::Sample {
            load: Some([2., 1., 0.5]),
            processes: Some(123),
            ..Default::default()
        });
        for width in [34, 54, 74] {
            let lines = system_lines(&app, width, 30);
            let line = lines
                .lines
                .iter()
                .find(|l| l.to_string().starts_with("LOAD"))
                .unwrap();
            assert_eq!(line.width(), width);
            assert!(line.to_string().ends_with("123 proc"));
            assert!(line.to_string().contains('↑'));
        }
    }
    #[test]
    fn provider_focus_remains_cyan_and_clickable_tabs_keep_python_palette() {
        let mut app = App::python_default().unwrap();
        app.agents = Some(crate::agent_app::tests::controller());
        app.select_tab('2');
        app.focus = hud_domain::PanelId::new("saida").unwrap();
        let mut terminal = Terminal::new(TestBackend::new(160, 50)).unwrap();
        terminal.draw(|frame| render(frame, &mut app)).unwrap();
        let output = app
            .rects
            .iter()
            .find(|p| p.id.as_str() == "saida")
            .unwrap()
            .rect;
        let border = &terminal.backend().buffer().content()
            [usize::from(output.row + 1) * 160 + usize::from(output.col)];
        assert_eq!(border.fg, Color::Cyan);
        assert!(border.modifier.contains(Modifier::BOLD));
        assert!(terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|c| c.fg == Color::Indexed(208) && c.modifier.contains(Modifier::REVERSED)));
        let hit = app
            .tab_hits
            .iter()
            .find(|(_, digit)| *digit == '3')
            .unwrap()
            .0;
        app.press(hud_domain::Cell {
            row: hit.row,
            col: hit.col,
        });
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Codex);
    }
}
