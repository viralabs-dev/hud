//! Bounded terminal Markdown; data never controls terminal escapes or opens links.
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
};
fn styled(text: impl Into<String>, style: Style) -> Span<'static> {
    Span::styled(text.into(), style)
}
fn inline(raw: &str, base: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut text = raw;
    while !text.is_empty() {
        if out.len() >= 256 {
            out.push(styled(text, base));
            break;
        }
        let markers = [
            ("`", Modifier::empty(), Color::Cyan),
            ("**", Modifier::BOLD, Color::Reset),
            ("__", Modifier::BOLD, Color::Reset),
            ("*", Modifier::ITALIC, Color::Reset),
            ("_", Modifier::ITALIC, Color::Reset),
        ];
        let next = markers
            .iter()
            .filter_map(|(mark, m, c)| text.find(mark).map(|i| (i, *mark, *m, *c)))
            .min_by_key(|(i, mark, _, _)| (*i, std::cmp::Reverse(mark.len())));
        let link = text.find('[').and_then(|start| {
            let after = &text[start + 1..];
            let close = after.find("](")?;
            let end = after[close + 2..].find(')')?;
            Some((start, close, end))
        });
        if let Some((start, close, end)) =
            link.filter(|(start, _, _)| next.as_ref().is_none_or(|(i, _, _, _)| start < i))
        {
            if start > 0 {
                out.push(styled(&text[..start], base));
            }
            let rest = &text[start + 1..];
            let label = &rest[..close];
            let url = &rest[close + 2..close + 2 + end];
            out.push(styled(label, base));
            if label != url {
                out.push(styled(format!(" ({url})"), base.fg(Color::DarkGray)));
            }
            text = &rest[close + 3 + end..];
            continue;
        }
        let Some((start, mark, modifier, color)) = next else {
            out.push(styled(text, base));
            break;
        };
        if start > 0 {
            out.push(styled(&text[..start], base));
        }
        let rest = &text[start + mark.len()..];
        if let Some(end) = rest.find(mark).filter(|end| *end > 0) {
            let style = if color == Color::Reset {
                base.add_modifier(modifier)
            } else {
                base.fg(color)
            };
            out.push(styled(&rest[..end], style));
            text = &rest[end + mark.len()..];
        } else {
            out.push(styled(mark, base));
            text = rest;
        }
    }
    out
}
fn wrap(spans: Vec<Span<'static>>, width: usize, hang: usize, budget: usize) -> Vec<Line<'static>> {
    if budget == 0 {
        return Vec::new();
    }
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut current = Vec::new();
    let mut used = 0;
    for span in spans {
        for character in span.content.chars() {
            let cell = character.to_string();
            let cells = Line::raw(cell.clone()).width();
            if used + cells > width && used > 0 {
                lines.push(Line::from(std::mem::take(&mut current)));
                if lines.len() == budget {
                    return lines;
                }
                let indent = hang.min(width.saturating_sub(1));
                current.push(Span::raw(" ".repeat(indent)));
                used = indent;
            }
            current.push(styled(cell, span.style));
            used += cells;
        }
    }
    lines.push(Line::from(current));
    lines
}
fn cells(raw: &str) -> Vec<String> {
    let raw = raw.trim().strip_prefix('|').unwrap_or(raw.trim());
    let raw = if raw.ends_with('|') && !raw.ends_with("\\|") {
        &raw[..raw.len() - 1]
    } else {
        raw
    };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut escaped = false;
    for c in raw.chars() {
        if c == '|' && !escaped {
            cells.push(cell.trim().into());
            cell.clear();
        } else {
            cell.push(c);
        }
        escaped = c == '\\';
    }
    cells.push(cell.trim().into());
    cells
}
fn separator(raw: &str) -> bool {
    raw.trim().starts_with('|') && raw.contains('-') && raw.chars().all(|c| "| :-\t".contains(c))
}
fn table(rows: Vec<Vec<String>>, width: usize, budget: usize) -> Vec<Line<'static>> {
    let plain = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| {
                    inline(cell, Style::default())
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let ncol = plain.iter().map(Vec::len).max().unwrap_or(0);
    let sizes = (0..ncol)
        .map(|col| {
            plain
                .iter()
                .map(|row| {
                    row.get(col)
                        .map_or(0, |cell| Line::raw(cell.as_str()).width())
                })
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let aligned = sizes.iter().sum::<usize>() + 3 * ncol.saturating_sub(1) <= width;
    let mut output = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if output.len() == budget {
            break;
        }
        let base = if index == 0 {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let mut spans = Vec::new();
        if aligned {
            for (col, size) in sizes.iter().enumerate() {
                if col > 0 {
                    spans.push(styled(" │ ", Style::default().fg(Color::DarkGray)));
                }
                let cell = plain[index].get(col).cloned().unwrap_or_default();
                let padding = size.saturating_sub(Line::raw(cell.as_str()).width());
                spans.push(styled(format!("{cell}{}", " ".repeat(padding)), base));
            }
        } else {
            spans.push(styled("• ", base));
            for (col, cell) in row.iter().enumerate() {
                if col > 0 {
                    spans.push(styled(" · ", Style::default().fg(Color::DarkGray)));
                }
                spans.extend(inline(cell, base));
            }
        }
        output.extend(wrap(
            spans,
            width,
            if aligned { 0 } else { 2 },
            budget - output.len(),
        ));
        if aligned && index == 0 && output.len() < budget {
            output.push(Line::styled(
                sizes
                    .iter()
                    .map(|size| "─".repeat(*size))
                    .collect::<Vec<_>>()
                    .join("─┼─"),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    output
}
/// Keeps physical wrapped lines for correct scroll offsets. Bounds input and output.
pub fn render(raw: &str, width: usize) -> Text<'static> {
    let safe = hud_resources::sanitize(&raw.chars().take(128 * 1024).collect::<String>());
    let input: Vec<_> = safe.lines().take(4096).collect();
    let mut lines = Vec::new();
    let mut i = 0;
    while i < input.len() && lines.len() < 8192 {
        let line = input[i];
        i += 1;
        let trimmed = line.trim_start();
        let base = Style::default();
        if trimmed.starts_with('|') && input.get(i).is_some_and(|next| separator(next)) {
            let mut rows = vec![cells(line)];
            i += 1;
            while i < input.len() && input[i].trim_start().starts_with('|') {
                rows.push(cells(input[i]));
                i += 1;
            }
            lines.extend(table(rows, width, 8192 - lines.len()));
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let mark = trimmed.chars().next().unwrap();
            let count = trimmed.chars().take_while(|c| *c == mark).count();
            let info = trimmed[count..].trim();
            let lang = info.split_whitespace().next().unwrap_or("");
            let mut body = Vec::new();
            while i < input.len() {
                let candidate = input[i].trim();
                i += 1;
                if candidate.chars().all(|c| c == mark) && candidate.len() >= count {
                    break;
                }
                body.push(input[i - 1]);
            }
            if matches!(lang, "hud-doc" | "hud-custom") {
                let file = info
                    .split("arquivo=")
                    .nth(1)
                    .and_then(|s| s.split_whitespace().next())
                    .unwrap_or("?")
                    .trim_matches(['\'', '"']);
                lines.push(Line::styled(
                    format!("  ▤ {file} · {} linha(s) · proposta de arquivo", body.len()),
                    base.fg(Color::DarkGray),
                ));
                continue;
            }
            if !lang.is_empty() {
                lines.push(Line::styled(
                    format!("  ┌ {lang}"),
                    base.fg(Color::DarkGray),
                ));
            }
            for code in body {
                lines.extend(wrap(
                    vec![
                        styled("  │ ", base.fg(Color::DarkGray)),
                        styled(code, base.fg(Color::Cyan)),
                    ],
                    width,
                    4,
                    8192 - lines.len(),
                ));
                if lines.len() == 8192 {
                    break;
                }
            }
            continue;
        }
        let heading = trimmed.chars().take_while(|c| *c == '#').count();
        let (spans, hang) =
            if heading > 0 && heading <= 6 && trimmed.as_bytes().get(heading) == Some(&b' ') {
                (
                    inline(
                        trimmed[heading + 1..].trim_end_matches('#').trim(),
                        base.fg(Color::Yellow).add_modifier(Modifier::BOLD),
                    ),
                    0,
                )
            } else if let Some(quote) = trimmed.strip_prefix('>') {
                let mut spans = vec![styled("▎ ", base.fg(Color::DarkGray))];
                spans.extend(inline(quote.trim_start(), base.fg(Color::Gray)));
                (spans, 2)
            } else if ["- ", "* ", "+ "]
                .iter()
                .any(|prefix| trimmed.starts_with(prefix))
            {
                let indent = line.len() - trimmed.len();
                let mut spans = vec![styled(format!("{}• ", " ".repeat(indent)), base)];
                spans.extend(inline(&trimmed[2..], base));
                (spans, indent + 2)
            } else if !trimmed.is_empty()
                && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3
                && ['-', '*', '_']
                    .iter()
                    .any(|mark| trimmed.chars().all(|c| c.is_whitespace() || c == *mark))
                || (trimmed.starts_with('|') && trimmed.chars().all(|c| "| :-".contains(c)))
            {
                (
                    vec![styled("─".repeat(width.min(24)), base.fg(Color::DarkGray))],
                    0,
                )
            } else {
                (inline(line, base), 0)
            };
        lines.extend(wrap(spans, width, hang, 8192 - lines.len()));
    }
    Text::from(lines)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn plain(text: &Text<'_>) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn tables_align_unicode_cells_or_stack_when_narrow() {
        let raw = "| Nome | Valor |\n| --- | --- |\n| 日本語 | **42** |\n| a\\|b | 1 |";
        let wide = plain(&render(raw, 80));
        assert!(wide.contains(" │ "));
        assert!(wide.contains("─┼─"));
        assert!(wide.contains("日本語"));
        assert!(wide.contains("a\\|b"));
        let narrow = render(raw, 10);
        assert!(plain(&narrow).contains(" · "));
        assert!(narrow.lines.iter().all(|line| line.width() <= 10));
    }
    #[test]
    fn adversarial_wrapping_obeys_physical_line_budget() {
        for raw in [
            "x".repeat(128 * 1024),
            format!("```rust\n{}\n```", "x".repeat(128 * 1024)),
        ] {
            assert_eq!(render(&raw, 1).lines.len(), 8192);
        }
        assert!(render(&"* ".repeat(65536), 1).lines.len() <= 8192);
    }
    #[test]
    fn headings_emphasis_code_links_quotes_and_sanitization() {
        let t=render("# Café\n**forte** e *leve* `**literal**` [Site](https://example.test)\n> ação\n- 日本語\n\x1b]52;c;secret\x07ok",120);
        let s = plain(&t);
        assert!(s.contains(
            "Café\nforte e leve **literal** Site (https://example.test)\n▎ ação\n• 日本語\nok"
        ));
        assert!(t.lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        assert!(!s.contains('\x1b'));
    }
    #[test]
    fn code_fences_and_proposals_do_not_render_secret_body_as_markdown() {
        let s = plain(&render(
            "```rust\n**literal**\n```\n```hud-doc arquivo=Nota.md\nbody\n```",
            80,
        ));
        assert!(s.contains("│ **literal**"));
        assert!(s.contains("Nota.md · 1 linha(s)"));
        assert!(!s.contains("body"));
    }
    #[test]
    fn unicode_wrapping_and_malformed_input_remain_visible() {
        let t = render("- 日本語ação\n**unclosed\n`x`", 6);
        assert!(t.lines.iter().all(|line| line.width() <= 6));
        let s = plain(&t);
        assert!(s.contains("**uncl"));
        assert_eq!(s.chars().filter(|c| *c == '日').count(), 1);
        assert!(plain(&render("", 1)).is_empty());
    }
}
