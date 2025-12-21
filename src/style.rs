//! CSS styling and theming
//!
//! Handles loading and applying CSS styles.

use anyhow::{Context, Result};
use gtk4::{gdk, CssProvider};
use std::path::Path;
use tracing::{debug, info, warn};

use crate::config::Config;

/// Default CSS styles
const DEFAULT_CSS: &str = r#"
/* Waygreet Default Theme */

/* Main window */
window {
    background-color: #1e1e2e;
    color: #cdd6f4;
}

/* Title styles */
.title-1 {
    font-size: 32px;
    font-weight: bold;
    margin-bottom: 16px;
}

.title-2 {
    font-size: 24px;
    font-weight: 600;
    margin-bottom: 8px;
}

/* Caption/label styles */
.caption {
    font-size: 12px;
    opacity: 0.8;
}

.dim-label {
    opacity: 0.6;
}

/* Error message */
.error {
    color: #f38ba8;
    font-weight: 500;
    padding: 8px 12px;
    background-color: rgba(243, 139, 168, 0.1);
    border-radius: 6px;
    margin-bottom: 8px;
}

/* Entry fields */
entry, password entry {
    padding: 12px 16px;
    border-radius: 8px;
    background-color: #313244;
    color: #cdd6f4;
    border: 2px solid transparent;
    font-size: 16px;
    min-height: 24px;
}

entry:focus, passwordentry:focus {
    border-color: #89b4fa;
    outline: none;
}

entry:disabled, passwordentry:disabled {
    opacity: 0.5;
}

/* Buttons */
button {
    padding: 12px 24px;
    border-radius: 8px;
    font-size: 16px;
    font-weight: 500;
    background-color: #45475a;
    color: #cdd6f4;
    border: none;
}

button:hover {
    background-color: #585b70;
}

button:active {
    background-color: #6c7086;
}

button:focus {
    outline: 2px solid #89b4fa;
    outline-offset: 2px;
}

button:disabled {
    opacity: 0.5;
}

/* Suggested action (primary button) */
button.suggested-action {
    background-color: #89b4fa;
    color: #1e1e2e;
}

button.suggested-action:hover {
    background-color: #b4befe;
}

/* Destructive action */
button.destructive-action {
    background-color: #f38ba8;
    color: #1e1e2e;
}

button.destructive-action:hover {
    background-color: #eba0ac;
}

/* Pill-shaped button */
button.pill {
    border-radius: 9999px;
    padding: 12px 32px;
}

/* Circular button */
button.circular {
    border-radius: 50%;
    padding: 12px;
    min-width: 48px;
    min-height: 48px;
}

/* Dropdown */
dropdown {
    padding: 8px 16px;
    border-radius: 8px;
    background-color: #313244;
    color: #cdd6f4;
}

dropdown:focus {
    outline: 2px solid #89b4fa;
}

/* Focus indicators for accessibility */
*:focus-visible {
    outline: 3px solid #89b4fa;
    outline-offset: 2px;
}
"#;

/// High contrast CSS
const HIGH_CONTRAST_CSS: &str = r#"
/* High Contrast Theme */

window {
    background-color: #000000;
    color: #ffffff;
}

.error {
    color: #ff6666;
    background-color: #330000;
    border: 2px solid #ff6666;
}

entry, passwordentry {
    background-color: #000000;
    color: #ffffff;
    border: 3px solid #ffffff;
}

entry:focus, passwordentry:focus {
    border-color: #00ffff;
}

button {
    background-color: #000000;
    color: #ffffff;
    border: 3px solid #ffffff;
}

button:hover {
    background-color: #333333;
}

button:focus {
    border-color: #00ffff;
    outline: none;
}

button.suggested-action {
    background-color: #ffffff;
    color: #000000;
}

button.destructive-action {
    background-color: #ff0000;
    color: #ffffff;
    border-color: #ff0000;
}

dropdown {
    background-color: #000000;
    color: #ffffff;
    border: 3px solid #ffffff;
}

*:focus-visible {
    outline: 4px solid #00ffff;
    outline-offset: 2px;
}
"#;

/// Load and apply CSS styles
pub fn load_css(config: &Config, style_override: Option<&Path>) -> Result<()> {
    let provider = CssProvider::new();

    // Determine which CSS to load
    let css_content = if config.appearance.high_contrast {
        info!("Using high contrast theme");
        HIGH_CONTRAST_CSS.to_string()
    } else if let Some(style_path) = style_override {
        load_css_file(style_path)?
    } else if !config.appearance.css_file.is_empty() {
        let path = Path::new(&config.appearance.css_file);
        if path.exists() {
            load_css_file(path)?
        } else {
            warn!("CSS file not found: {:?}, using default", path);
            DEFAULT_CSS.to_string()
        }
    } else {
        DEFAULT_CSS.to_string()
    };

    // Apply font scaling
    let css_with_scaling = apply_font_scaling(&css_content, config.appearance.font_scale);

    // Load the CSS
    provider.load_from_string(&css_with_scaling);

    // Apply to all displays
    if let Some(display) = gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        info!("CSS styles loaded");
    } else {
        warn!("No display available for CSS");
    }

    Ok(())
}

/// Load CSS from a file
fn load_css_file(path: &Path) -> Result<String> {
    debug!("Loading CSS from {:?}", path);

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read CSS file: {:?}", path))?;

    Ok(content)
}

/// Apply font scaling to CSS
fn apply_font_scaling(css: &str, scale: f64) -> String {
    if (scale - 1.0).abs() < 0.01 {
        return css.to_string();
    }

    // Simple approach: add a root font-size rule
    let font_size = (16.0 * scale).round() as u32;

    format!(
        "/* Font scale: {} */\n* {{ font-size: {}px; }}\n\n{}",
        scale, font_size, css
    )
}

/// Get the default CSS content
#[allow(dead_code)]
pub fn get_default_css() -> &'static str {
    DEFAULT_CSS
}

/// Get the high contrast CSS content
#[allow(dead_code)]
pub fn get_high_contrast_css() -> &'static str {
    HIGH_CONTRAST_CSS
}
