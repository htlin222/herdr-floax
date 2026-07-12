//! Floating-box geometry and rendering: a dimmed backdrop, a centered rounded
//! box sized by the configured percentages, and the embedded vt100 screen
//! painted into the box interior.

use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Clear};

use crate::config::{BorderKind, Config};
use crate::snapshot::Snapshot;

/// The floating box (border included), centered in `area` per the configured
/// percentages. Width/height are clamped so tiny panes still get a usable box.
pub fn box_rect(area: Rect, cfg: &Config) -> Rect {
    let w = ((u32::from(area.width) * u32::from(cfg.width_pct)) / 100) as u16;
    let h = ((u32::from(area.height) * u32::from(cfg.height_pct)) / 100) as u16;
    let w = w.clamp(20.min(area.width), area.width);
    let h = h.clamp(5.min(area.height), area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

/// The PTY-facing interior of the floating box (inside the 1-cell border).
/// This is what the embedded shell believes its terminal size is.
pub fn box_inner(area: Rect, cfg: &Config) -> Rect {
    let r = box_rect(area, cfg);
    Rect::new(
        r.x + 1,
        r.y + 1,
        r.width.saturating_sub(2).max(1),
        r.height.saturating_sub(2).max(1),
    )
}

/// Draw one frame: backdrop, box chrome, embedded screen, cursor.
pub fn draw(f: &mut Frame, cfg: &Config, screen: &vt100::Screen, snap: Option<&Snapshot>) {
    let area = f.area();

    // Backdrop. With a snapshot (captured by the toggle script) the real
    // workspace is painted behind the box with only its TEXT dimmed — every
    // background color (including default/terminal) passes through
    // untouched, like a tmux popup. Without a snapshot, fall back to the
    // quiet dark fill (`backdrop` config color).
    if let Some(snap) = snap {
        render_snapshot_dimmed(f.buffer_mut(), area, snap);
    } else {
        let (br, bg, bb) = cfg.backdrop;
        f.render_widget(
            Block::default()
                .style(Style::default().bg(Color::Rgb(br, bg, bb)).fg(Color::DarkGray)),
            area,
        );
    }

    let boxr = box_rect(area, cfg);
    f.render_widget(Clear, boxr);
    let border_color = cfg
        .border
        .map_or(Color::Magenta, |(r, g, b)| Color::Rgb(r, g, b));
    let box_bg = cfg.box_bg.map_or(Color::Reset, |(r, g, b)| Color::Rgb(r, g, b));
    let chrome = Block::bordered()
        .style(Style::default().bg(box_bg))
        .border_type(match cfg.border_type {
            BorderKind::Plain => BorderType::Plain,
            BorderKind::Rounded => BorderType::Rounded,
        })
        .border_style(Style::default().fg(border_color))
        .title(
            Line::from(format!(
                " ⌂ {} ",
                if cfg.title.is_empty() { "floax" } else { &cfg.title }
            ))
            .centered(),
        )
        .title_bottom(
            Line::from(format!(" {} hides · shell persists ", cfg.key_hint))
                .centered()
                .style(Style::default().fg(Color::DarkGray)),
        );
    let inner = chrome.inner(boxr);
    f.render_widget(chrome, boxr);

    render_screen(f.buffer_mut(), inner, screen, box_bg);

    if !screen.hide_cursor() {
        let (r, c) = screen.cursor_position();
        if r < inner.height && c < inner.width {
            f.set_cursor_position(Position::new(inner.x + c, inner.y + r));
        }
    }
}

/// Paint the vt100 screen's cells into the buffer at `area`'s offset.
/// `default_bg` replaces default-background cells so the box interior stays
/// one consistent color instead of falling through to the host terminal.
fn render_screen(buf: &mut Buffer, area: Rect, screen: &vt100::Screen, default_bg: Color) {
    let (rows, cols) = screen.size();
    for r in 0..rows.min(area.height) {
        let mut skip_next = false; // second half of a wide (CJK/emoji) cell
        for c in 0..cols.min(area.width) {
            if skip_next {
                skip_next = false;
                continue;
            }
            let Some(cell) = screen.cell(r, c) else { continue };
            let Some(target) = buf.cell_mut(Position::new(area.x + c, area.y + r)) else {
                continue;
            };
            let contents = cell.contents();
            target.set_symbol(if contents.is_empty() { " " } else { &contents });
            let bg = match cell.bgcolor() {
                vt100::Color::Default => default_bg,
                other => conv_color(other),
            };
            let mut style = Style::default().fg(conv_color(cell.fgcolor())).bg(bg);
            if cell.bold() {
                style = style.add_modifier(Modifier::BOLD);
            }
            if cell.italic() {
                style = style.add_modifier(Modifier::ITALIC);
            }
            if cell.underline() {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if cell.inverse() {
                style = style.add_modifier(Modifier::REVERSED);
            }
            target.set_style(style);
            skip_next = cell.is_wide();
        }
    }
}

fn conv_color(c: vt100::Color) -> Color {
    match c {
        vt100::Color::Default => Color::Reset,
        vt100::Color::Idx(i) => Color::Indexed(i),
        vt100::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// Paint the captured workspace panes into the backdrop with dimmed text.
/// Background colors are passed through UNCHANGED (default stays default) so
/// the margin looks exactly like the real workspace, only quieter. Snapshot
/// coordinates are absolute client cells; the plugin pane is opened as the
/// only pane of a fresh tab (`--placement tab`), which herdr renders
/// borderless over the full tab area — the same region the captured tab's
/// panes occupied — so cells map by subtracting the AREA origin, no border
/// inset to compensate.
fn render_snapshot_dimmed(buf: &mut Buffer, area: Rect, snap: &Snapshot) {
    for pane in &snap.panes {
        let ox = i32::from(pane.rect.x) - i32::from(snap.area.x) + i32::from(area.x);
        let oy = i32::from(pane.rect.y) - i32::from(snap.area.y) + i32::from(area.y);
        let (rows, cols) = pane.screen.size();
        for r in 0..rows.min(pane.rect.height) {
            let mut skip_next = false;
            for c in 0..cols.min(pane.rect.width) {
                if skip_next {
                    skip_next = false;
                    continue;
                }
                let Some(cell) = pane.screen.cell(r, c) else { continue };
                skip_next = cell.is_wide();
                let (x, y) = (ox + i32::from(c), oy + i32::from(r));
                if x < i32::from(area.x)
                    || y < i32::from(area.y)
                    || x >= i32::from(area.right())
                    || y >= i32::from(area.bottom())
                {
                    continue;
                }
                let Some(target) = buf.cell_mut(Position::new(x as u16, y as u16)) else {
                    continue;
                };
                let contents = cell.contents();
                target.set_symbol(if contents.is_empty() { " " } else { &contents });
                target.set_style(
                    Style::default()
                        .fg(dim_fg(cell.fgcolor()))
                        .bg(conv_color(cell.bgcolor())),
                );
            }
        }
    }
}

/// Dim a foreground color to ~40% so the backdrop reads as inactive.
fn dim_fg(c: vt100::Color) -> Color {
    let (r, g, b) = match c {
        vt100::Color::Default => (205, 214, 244), // assume a soft-white text default
        vt100::Color::Idx(i) => idx_to_rgb(i),
        vt100::Color::Rgb(r, g, b) => (r, g, b),
    };
    scale_rgb(r, g, b, 2, 5) // * 0.4
}

fn scale_rgb(r: u8, g: u8, b: u8, num: u16, den: u16) -> Color {
    let s = |v: u8| (u16::from(v) * num / den) as u8;
    Color::Rgb(s(r), s(g), s(b))
}

/// xterm 256-color index → RGB (standard palette; 0–15 use common defaults).
fn idx_to_rgb(i: u8) -> (u8, u8, u8) {
    const BASE16: [(u8, u8, u8); 16] = [
        (0, 0, 0),
        (205, 49, 49),
        (13, 188, 121),
        (229, 229, 16),
        (36, 114, 200),
        (188, 63, 188),
        (17, 168, 205),
        (229, 229, 229),
        (102, 102, 102),
        (241, 76, 76),
        (35, 209, 139),
        (245, 245, 67),
        (59, 142, 234),
        (214, 112, 214),
        (41, 184, 219),
        (255, 255, 255),
    ];
    match i {
        0..=15 => BASE16[usize::from(i)],
        16..=231 => {
            let n = i - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(w: u16, h: u16) -> Config {
        Config { width_pct: w, height_pct: h, ..Config::default() }
    }

    #[test]
    fn box_is_centered_at_80pct() {
        let r = box_rect(Rect::new(0, 0, 100, 50), &cfg(80, 80));
        assert_eq!(r, Rect::new(10, 5, 80, 40));
    }

    #[test]
    fn box_respects_area_offset() {
        let r = box_rect(Rect::new(7, 3, 100, 50), &cfg(80, 80));
        assert_eq!(r, Rect::new(17, 8, 80, 40));
    }

    #[test]
    fn box_clamps_to_minimums_on_tiny_areas() {
        let r = box_rect(Rect::new(0, 0, 30, 6), &cfg(20, 20));
        assert!(r.width >= 20 && r.height >= 5);
        // ... and never exceeds the area even when the area is below minimums.
        let r = box_rect(Rect::new(0, 0, 12, 3), &cfg(80, 80));
        assert!(r.width <= 12 && r.height <= 3);
    }

    #[test]
    fn full_size_at_100pct() {
        let area = Rect::new(0, 0, 100, 50);
        assert_eq!(box_rect(area, &cfg(100, 100)), area);
    }

    #[test]
    fn inner_is_border_inset_and_never_zero() {
        let inner = box_inner(Rect::new(0, 0, 100, 50), &cfg(80, 80));
        assert_eq!(inner, Rect::new(11, 6, 78, 38));
        let tiny = box_inner(Rect::new(0, 0, 2, 2), &cfg(20, 20));
        assert!(tiny.width >= 1 && tiny.height >= 1);
    }
}
