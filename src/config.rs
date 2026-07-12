//! Floating-box configuration: size percentages and the key hint shown in the
//! bottom border.
//!
//! Precedence: built-in defaults < `floax.conf` in `$HERDR_PLUGIN_CONFIG_DIR`
//! (KEY=VALUE lines) < `HERDR_FLOAX_*` environment variables. The env layer
//! lets the toggle script (or the user) override per-invocation without
//! touching the config file.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Box width as a percentage of the pane, clamped to 20..=100.
    pub width_pct: u16,
    /// Box height as a percentage of the pane, clamped to 20..=100.
    pub height_pct: u16,
    /// The toggle key shown in the bottom-border hint (display only).
    pub key_hint: String,
    /// Backdrop fill color (RGB), drawn around the floating box.
    pub backdrop: (u8, u8, u8),
    /// Border color (RGB). `None` keeps the terminal-palette magenta default.
    pub border: Option<(u8, u8, u8)>,
    /// Border corner style.
    pub border_type: BorderKind,
    /// Title shown in the top border. Empty means auto: the basename of
    /// `$HERDR_FLOAX_CWD` (the directory the shell starts in), falling back
    /// to "floax".
    pub title: String,
    /// Box background (RGB) painted wherever the embedded screen has no
    /// explicit background — border rows included. `None` passes default
    /// backgrounds through (the host terminal decides, which can clash with
    /// the multiplexer theme).
    pub box_bg: Option<(u8, u8, u8)>,
}

/// Corner style for the floating box border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderKind {
    Rounded,
    Plain,
}

impl Default for Config {
    fn default() -> Self {
        // Generous defaults: the backdrop is a dead dark fill (see README
        // "Limitations" — it can't show the real workspace dimmed), so margin
        // is wasted space. Keep just enough inset to read as a floating box.
        // Backdrop: a deep green (#0d2b1d).
        Self {
            width_pct: 94,
            height_pct: 92,
            key_hint: "prefix+f".into(),
            backdrop: (0x0d, 0x2b, 0x1d),
            border: None,
            border_type: BorderKind::Rounded,
            title: String::new(),
            box_bg: None,
        }
    }
}

impl Config {
    /// Resolve config from the plugin config dir and the process environment.
    pub fn load() -> Self {
        let mut cfg = Self::default();
        if let Ok(dir) = std::env::var("HERDR_PLUGIN_CONFIG_DIR") {
            if let Ok(text) = std::fs::read_to_string(format!("{dir}/floax.conf")) {
                cfg.apply_conf(&text);
            }
        }
        cfg.apply_env(|k| std::env::var(k).ok());
        if cfg.title.is_empty() {
            cfg.title = std::env::var("HERDR_FLOAX_CWD")
                .ok()
                .as_deref()
                .and_then(|d| std::path::Path::new(d).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "floax".into());
        }
        cfg
    }

    /// Apply `KEY=VALUE` lines (`#` comments and blank lines ignored).
    pub fn apply_conf(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                self.set(k.trim(), v.trim());
            }
        }
    }

    /// Apply `HERDR_FLOAX_*` overrides from an env lookup (injected for tests).
    pub fn apply_env(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(v) = get("HERDR_FLOAX_WIDTH_PCT") {
            self.set("width_pct", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_HEIGHT_PCT") {
            self.set("height_pct", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_KEY_HINT") {
            self.set("key_hint", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_BACKDROP") {
            self.set("backdrop", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_BORDER") {
            self.set("border", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_BORDER_TYPE") {
            self.set("border_type", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_TITLE") {
            self.set("title", &v);
        }
        if let Some(v) = get("HERDR_FLOAX_BOX_BG") {
            self.set("box_bg", &v);
        }
    }

    fn set(&mut self, key: &str, val: &str) {
        match key {
            "width_pct" => {
                if let Ok(n) = val.parse::<u16>() {
                    self.width_pct = clamp_pct(n);
                }
            }
            "height_pct" => {
                if let Ok(n) = val.parse::<u16>() {
                    self.height_pct = clamp_pct(n);
                }
            }
            "key_hint" => {
                if !val.is_empty() {
                    self.key_hint = val.to_string();
                }
            }
            "backdrop" => {
                if let Some(rgb) = parse_hex_color(val) {
                    self.backdrop = rgb;
                }
            }
            "border" => {
                if let Some(rgb) = parse_hex_color(val) {
                    self.border = Some(rgb);
                }
            }
            "border_type" => match val {
                "plain" => self.border_type = BorderKind::Plain,
                "rounded" => self.border_type = BorderKind::Rounded,
                _ => {}
            },
            "title" => {
                if !val.is_empty() {
                    self.title = val.to_string();
                }
            }
            "box_bg" => {
                if let Some(rgb) = parse_hex_color(val) {
                    self.box_bg = Some(rgb);
                }
            }
            _ => {}
        }
    }
}

fn clamp_pct(n: u16) -> u16 {
    n.clamp(20, 100)
}

/// Parse `#rrggbb` (leading `#` optional) into an RGB triple.
fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let hex = s.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let c = Config::default();
        assert_eq!((c.width_pct, c.height_pct), (94, 92));
    }

    #[test]
    fn conf_lines_parse_with_comments_and_junk() {
        let mut c = Config::default();
        c.apply_conf("# floax\nwidth_pct = 60\n\nheight_pct=45\nnot a kv line\nunknown=1\n");
        assert_eq!((c.width_pct, c.height_pct), (60, 45));
    }

    #[test]
    fn pct_clamped_and_bad_values_ignored() {
        let mut c = Config::default();
        c.apply_conf("width_pct=5\nheight_pct=999\n");
        assert_eq!((c.width_pct, c.height_pct), (20, 100));
        c.apply_conf("width_pct=banana\n");
        assert_eq!(c.width_pct, 20); // unchanged by unparsable value
    }

    #[test]
    fn env_overrides_conf() {
        let mut c = Config::default();
        c.apply_conf("width_pct=60\n");
        c.apply_env(|k| (k == "HERDR_FLOAX_WIDTH_PCT").then(|| "90".to_string()));
        assert_eq!(c.width_pct, 90);
    }

    #[test]
    fn key_hint_set_and_empty_ignored() {
        let mut c = Config::default();
        c.apply_conf("key_hint=prefix+g\nkey_hint=\n");
        assert_eq!(c.key_hint, "prefix+g");
    }

    #[test]
    fn border_and_title_keys_parse() {
        let mut c = Config::default();
        c.apply_conf("border = #89b4fa\nborder_type = plain\ntitle = mybox\n");
        assert_eq!(c.border, Some((0x89, 0xb4, 0xfa)));
        assert_eq!(c.border_type, BorderKind::Plain);
        assert_eq!(c.title, "mybox");
        c.apply_conf("border_type = wiggly\nborder = nope\ntitle =\n");
        assert_eq!(c.border, Some((0x89, 0xb4, 0xfa))); // bad values ignored
        assert_eq!(c.border_type, BorderKind::Plain);
        assert_eq!(c.title, "mybox");
    }

    #[test]
    fn backdrop_defaults_to_dark_green() {
        assert_eq!(Config::default().backdrop, (0x0d, 0x2b, 0x1d));
    }

    #[test]
    fn backdrop_parses_hex_with_and_without_hash() {
        let mut c = Config::default();
        c.apply_conf("backdrop = #102030\n");
        assert_eq!(c.backdrop, (0x10, 0x20, 0x30));
        c.apply_conf("backdrop = A1B2C3\n");
        assert_eq!(c.backdrop, (0xa1, 0xb2, 0xc3));
    }

    #[test]
    fn backdrop_bad_values_ignored() {
        let mut c = Config::default();
        for bad in ["#12345", "#1234567", "not-a-color", "#zzzzzz", ""] {
            c.apply_conf(&format!("backdrop={bad}\n"));
            assert_eq!(c.backdrop, Config::default().backdrop, "should ignore {bad:?}");
        }
    }

    #[test]
    fn backdrop_env_override() {
        let mut c = Config::default();
        c.apply_env(|k| (k == "HERDR_FLOAX_BACKDROP").then(|| "#334455".to_string()));
        assert_eq!(c.backdrop, (0x33, 0x44, 0x55));
    }
}
