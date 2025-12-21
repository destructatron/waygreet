//! Session discovery
//!
//! Discovers available login sessions from desktop entry files.

use anyhow::{Context, Result};
use freedesktop_entry_parser::parse_entry;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use crate::config::SessionsConfig;

/// Session type (Wayland or X11)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionType {
    Wayland,
    X11,
}

impl std::fmt::Display for SessionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionType::Wayland => write!(f, "Wayland"),
            SessionType::X11 => write!(f, "X11"),
        }
    }
}

/// A login session
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Session {
    /// Display name
    pub name: String,
    /// Executable command
    pub exec: Vec<String>,
    /// Session type
    pub session_type: SessionType,
    /// Description/comment
    pub comment: Option<String>,
    /// Desktop names (XDG_CURRENT_DESKTOP value)
    pub desktop_names: Option<String>,
    /// Icon name
    pub icon: Option<String>,
    /// Source file path
    pub file_path: PathBuf,
}

impl Session {
    /// Get the command to execute for this session
    pub fn get_command(&self, config: &SessionsConfig) -> Vec<String> {
        match self.session_type {
            SessionType::Wayland => self.exec.clone(),
            SessionType::X11 => {
                // Wrap X11 sessions with the configured wrapper
                let mut cmd = config.x11_wrapper.clone();
                cmd.extend(self.exec.clone());
                cmd
            }
        }
    }

    /// Get environment variables for this session
    pub fn get_env(&self) -> Vec<String> {
        let mut env = vec![];

        // Set XDG_SESSION_TYPE
        match self.session_type {
            SessionType::Wayland => {
                env.push("XDG_SESSION_TYPE=wayland".to_string());
            }
            SessionType::X11 => {
                env.push("XDG_SESSION_TYPE=x11".to_string());
            }
        }

        // Set XDG_CURRENT_DESKTOP if available
        if let Some(ref desktop) = self.desktop_names {
            env.push(format!("XDG_CURRENT_DESKTOP={}", desktop));
        }

        env
    }
}

/// Discover all available sessions
pub fn discover_sessions(config: &SessionsConfig) -> Vec<Session> {
    let mut sessions = vec![];

    // Standard session directories
    let wayland_dirs = ["/usr/share/wayland-sessions"];
    let x11_dirs = ["/usr/share/xsessions"];

    // Discover Wayland sessions
    if config.show_wayland {
        for dir in &wayland_dirs {
            if let Ok(discovered) = discover_sessions_in_dir(Path::new(dir), SessionType::Wayland) {
                sessions.extend(discovered);
            }
        }
    }

    // Discover X11 sessions
    if config.show_x11 {
        for dir in &x11_dirs {
            if let Ok(discovered) = discover_sessions_in_dir(Path::new(dir), SessionType::X11) {
                sessions.extend(discovered);
            }
        }
    }

    // Discover sessions from extra directories
    for dir in &config.extra_dirs {
        // Guess session type from directory name
        let session_type = if dir.contains("wayland") {
            SessionType::Wayland
        } else {
            SessionType::X11
        };

        if let Ok(discovered) = discover_sessions_in_dir(Path::new(dir), session_type) {
            sessions.extend(discovered);
        }
    }

    // Sort by name
    sessions.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Remove duplicates by name
    sessions.dedup_by(|a, b| a.name == b.name);

    debug!("Discovered {} sessions", sessions.len());

    sessions
}

/// Discover sessions in a specific directory
fn discover_sessions_in_dir(dir: &Path, session_type: SessionType) -> Result<Vec<Session>> {
    let mut sessions = vec![];

    if !dir.exists() {
        debug!("Session directory {:?} does not exist", dir);
        return Ok(sessions);
    }

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("Failed to read session directory: {:?}", dir))?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .desktop files
        if path.extension().map(|e| e != "desktop").unwrap_or(true) {
            continue;
        }

        match parse_desktop_entry(&path, session_type.clone()) {
            Ok(Some(session)) => {
                debug!("Found session: {} ({:?})", session.name, session.session_type);
                sessions.push(session);
            }
            Ok(None) => {
                debug!("Skipping hidden/nodisplay session: {:?}", path);
            }
            Err(e) => {
                warn!("Failed to parse session file {:?}: {}", path, e);
            }
        }
    }

    Ok(sessions)
}

/// Parse a desktop entry file into a Session
fn parse_desktop_entry(path: &Path, session_type: SessionType) -> Result<Option<Session>> {
    let entry = parse_entry(path)
        .with_context(|| format!("Failed to parse: {:?}", path))?;

    // Get the Desktop Entry section
    let section = entry
        .section("Desktop Entry");

    let section = match section.attr("Type") {
        Some(_) => section,
        None => return Ok(None), // Not a valid desktop entry
    };

    // Check if it's a valid session
    let entry_type = section.attr("Type").unwrap_or("Application");
    if entry_type != "Application" && entry_type != "XSession" {
        return Ok(None);
    }

    // Skip hidden entries
    if section.attr("Hidden").unwrap_or("false") == "true" {
        return Ok(None);
    }

    // Skip NoDisplay entries
    if section.attr("NoDisplay").unwrap_or("false") == "true" {
        return Ok(None);
    }

    // Get required fields
    let name = match section.attr("Name") {
        Some(n) => n.to_string(),
        None => return Ok(None), // Skip entries without Name
    };

    let exec_str = match section.attr("Exec").or_else(|| section.attr("TryExec")) {
        Some(e) => e,
        None => return Ok(None), // Skip entries without Exec
    };

    // Parse the Exec command
    let exec = parse_exec(exec_str);

    // Get optional fields
    let comment = section.attr("Comment").map(|s| s.to_string());
    let desktop_names = section.attr("DesktopNames").map(|s| s.to_string());
    let icon = section.attr("Icon").map(|s| s.to_string());

    Ok(Some(Session {
        name,
        exec,
        session_type,
        comment,
        desktop_names,
        icon,
        file_path: path.to_path_buf(),
    }))
}

/// Parse an Exec string into command arguments
fn parse_exec(exec: &str) -> Vec<String> {
    // Simple parsing - split by whitespace but handle basic quoting
    let mut args = vec![];
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = ' ';

    for c in exec.chars() {
        match c {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = c;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    // Skip desktop entry field codes like %f, %F, %u, %U, etc.
                    if !current.starts_with('%') || current.len() != 2 {
                        args.push(current.clone());
                    }
                    current.clear();
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() && (!current.starts_with('%') || current.len() != 2) {
        args.push(current);
    }

    args
}

/// Find a session by name
#[allow(dead_code)]
pub fn find_session_by_name<'a>(sessions: &'a [Session], name: &str) -> Option<&'a Session> {
    sessions.iter().find(|s| s.name == name)
}

/// Get the default session (first Wayland session, or first session)
#[allow(dead_code)]
pub fn get_default_session<'a>(sessions: &'a [Session], config: &SessionsConfig) -> Option<&'a Session> {
    // If a default is configured, try to find it
    if !config.default_session.is_empty() {
        if let Some(session) = find_session_by_name(sessions, &config.default_session) {
            return Some(session);
        }
    }

    // Prefer Wayland sessions
    if let Some(session) = sessions.iter().find(|s| s.session_type == SessionType::Wayland) {
        return Some(session);
    }

    // Fall back to first session
    sessions.first()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exec() {
        assert_eq!(parse_exec("sway"), vec!["sway"]);
        assert_eq!(parse_exec("gnome-session"), vec!["gnome-session"]);
        assert_eq!(
            parse_exec("dbus-run-session gnome-session"),
            vec!["dbus-run-session", "gnome-session"]
        );
        // Field codes should be stripped
        assert_eq!(parse_exec("firefox %u"), vec!["firefox"]);
    }
}
