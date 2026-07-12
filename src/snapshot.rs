//! Workspace snapshot: the real panes behind the popup, captured by the
//! toggle script just before it opens the floating pane, so the backdrop can
//! show the actual workspace dimmed (à la tmux popup) instead of a dead fill.
//!
//! The script writes a simple line format to a temp file and passes its path
//! via `HERDR_FLOAX_SNAPSHOT`:
//!
//! ```text
//! AREA <x> <y> <w> <h>          # the tab's pane area, client coordinates
//! PANE <x> <y> <w> <h>          # one per pane, client coordinates
//! <raw ANSI screen lines…>      # `herdr pane read --format ansi` output
//! FLOAX_END_PANE
//! ```
//!
//! Coordinates are absolute client cells; `draw` maps them into the plugin
//! pane by subtracting the AREA origin (the zoomed plugin pane covers the
//! same region the tab's panes did). The snapshot is static — it is the
//! workspace as it looked at open time, which is all a plugin pane can get.

use ratatui::layout::Rect;

pub struct PaneSnap {
    /// Pane content rect in client coordinates.
    pub rect: Rect,
    /// Parsed screen contents (fed through vt100 at the pane's size).
    pub screen: vt100::Screen,
}

pub struct Snapshot {
    /// The tab's pane area in client coordinates (AREA line).
    pub area: Rect,
    pub panes: Vec<PaneSnap>,
}

impl Snapshot {
    /// Load from the file named by `HERDR_FLOAX_SNAPSHOT`, if present/valid.
    pub fn load_from_env() -> Option<Self> {
        let path = std::env::var("HERDR_FLOAX_SNAPSHOT").ok()?;
        if path.is_empty() {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        Self::parse(&bytes)
    }

    fn parse(bytes: &[u8]) -> Option<Self> {
        let mut lines = bytes.split(|&b| b == b'\n');
        let area = parse_rect_line(lines.next()?, b"AREA ")?;
        let mut panes = Vec::new();
        while let Some(line) = lines.next() {
            let Some(rect) = parse_rect_line(line, b"PANE ") else { continue };
            let mut parser =
                vt100::Parser::new(rect.height.max(1), rect.width.max(1), 0);
            let mut first = true;
            for content in lines.by_ref() {
                let content = content.strip_suffix(b"\r").unwrap_or(content);
                // `herdr pane read` output has no trailing newline, so the
                // end marker can land glued onto the last content line —
                // accept it as a suffix, render the prefix, and stop.
                let (body, is_end) = match content.strip_suffix(b"FLOAX_END_PANE") {
                    Some(body) => (body, true),
                    None => (content, false),
                };
                if !(is_end && body.is_empty()) {
                    // Newlines go BETWEEN lines: a trailing one on the last
                    // row would scroll the screen (scrollback is 0) and lose
                    // a line.
                    if !first {
                        parser.process(b"\r\n");
                    }
                    first = false;
                    parser.process(body);
                }
                if is_end {
                    break;
                }
            }
            panes.push(PaneSnap { rect, screen: parser.screen().clone() });
        }
        (!panes.is_empty()).then_some(Self { area, panes })
    }
}

/// Parse `<prefix><x> <y> <w> <h>` into a Rect; None on any mismatch.
fn parse_rect_line(line: &[u8], prefix: &[u8]) -> Option<Rect> {
    let rest = std::str::from_utf8(line.strip_prefix(prefix)?).ok()?;
    let mut it = rest.split_whitespace().map(|n| n.parse::<u16>());
    let (x, y, w, h) =
        (it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?);
    Some(Rect::new(x, y, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_area_and_panes() {
        let text = b"AREA 25 1 207 42\nPANE 25 1 100 2\nhello\nworld\nFLOAX_END_PANE\nPANE 126 1 81 1\nbye\nFLOAX_END_PANE\n";
        let s = Snapshot::parse(text).expect("parses");
        assert_eq!(s.area, Rect::new(25, 1, 207, 42));
        assert_eq!(s.panes.len(), 2);
        assert_eq!(s.panes[0].rect, Rect::new(25, 1, 100, 2));
        let cell = s.panes[0].screen.cell(0, 0).expect("cell");
        assert_eq!(cell.contents(), "h");
        let cell = s.panes[0].screen.cell(1, 0).expect("cell");
        assert_eq!(cell.contents(), "w");
    }

    #[test]
    fn end_marker_glued_to_last_line_still_terminates_pane() {
        // `herdr pane read` emits no trailing newline, so the script's echo
        // lands on the same line as the last row of content.
        let text = b"AREA 0 0 30 5\nPANE 0 0 30 2\nrow one\nrow twoFLOAX_END_PANE\nPANE 0 3 30 1\nnextFLOAX_END_PANE\n";
        let s = Snapshot::parse(text).expect("parses");
        assert_eq!(s.panes.len(), 2);
        assert_eq!(s.panes[0].screen.cell(1, 4).expect("cell").contents(), "t");
        assert_eq!(s.panes[1].screen.cell(0, 0).expect("cell").contents(), "n");
    }

    #[test]
    fn empty_or_garbage_yields_none() {
        assert!(Snapshot::parse(b"").is_none());
        assert!(Snapshot::parse(b"not a snapshot").is_none());
        assert!(Snapshot::parse(b"AREA 0 0 10 10\n").is_none()); // no panes
    }

    #[test]
    fn ansi_colors_survive_parsing() {
        let text = b"AREA 0 0 20 2\nPANE 0 0 20 1\n\x1b[38;2;10;20;30mX\nFLOAX_END_PANE\n";
        let s = Snapshot::parse(text).expect("parses");
        let cell = s.panes[0].screen.cell(0, 0).expect("cell");
        assert_eq!(cell.fgcolor(), vt100::Color::Rgb(10, 20, 30));
    }
}
