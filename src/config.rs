//! Configuration loading and management

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main configuration structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub accessibility: AccessibilityConfig,
    pub sessions: SessionsConfig,
    pub appearance: AppearanceConfig,
    pub behavior: BehaviorConfig,
    pub commands: CommandsConfig,
    pub environment: EnvironmentConfig,
}

/// Accessibility configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AccessibilityConfig {
    /// Start Orca screen reader by default
    pub start_orca: bool,
    /// Use systemd for Orca if available
    pub prefer_systemd: bool,
    /// Path to orca executable
    pub orca_path: String,
    /// Additional Orca arguments
    pub orca_args: Vec<String>,
    /// Enable audio (PipeWire)
    pub enable_audio: bool,
    /// AT-SPI environment setup
    pub setup_atspi: bool,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            start_orca: true,
            prefer_systemd: true,
            orca_path: "/usr/bin/orca".to_string(),
            orca_args: vec!["--replace".to_string()],
            enable_audio: true,
            setup_atspi: true,
        }
    }
}

/// Session discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionsConfig {
    /// Additional session directories
    pub extra_dirs: Vec<String>,
    /// X11 session wrapper command
    pub x11_wrapper: Vec<String>,
    /// Default session name
    pub default_session: String,
    /// Show X11 sessions
    pub show_x11: bool,
    /// Show Wayland sessions
    pub show_wayland: bool,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self {
            extra_dirs: vec![],
            x11_wrapper: vec!["startx".to_string()],
            default_session: String::new(),
            show_x11: true,
            show_wayland: true,
        }
    }
}

/// Appearance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceConfig {
    /// GTK theme name (empty for system default)
    pub theme: String,
    /// Enable dark mode
    pub dark_mode: bool,
    /// Custom CSS file path
    pub css_file: String,
    /// High contrast mode
    pub high_contrast: bool,
    /// Font scale factor
    pub font_scale: f64,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            theme: String::new(),
            dark_mode: true,
            css_file: String::new(),
            high_contrast: false,
            font_scale: 1.0,
        }
    }
}

/// Behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BehaviorConfig {
    /// Remember last user
    pub remember_user: bool,
    /// Remember last session
    pub remember_session: bool,
    /// State file path
    pub state_file: String,
    /// Initial focus field
    pub initial_focus: String,
    /// Show clock
    pub show_clock: bool,
    /// Clock format
    pub clock_format: String,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            remember_user: true,
            remember_session: true,
            state_file: "/var/cache/waygreet/state.toml".to_string(),
            initial_focus: "username".to_string(),
            show_clock: true,
            clock_format: "%H:%M".to_string(),
        }
    }
}

/// Command configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandsConfig {
    /// Reboot command
    pub reboot: Vec<String>,
    /// Shutdown command
    pub shutdown: Vec<String>,
    /// Enable power menu
    pub enable_power_menu: bool,
}

impl Default for CommandsConfig {
    fn default() -> Self {
        Self {
            reboot: vec!["systemctl".to_string(), "reboot".to_string()],
            shutdown: vec!["systemctl".to_string(), "poweroff".to_string()],
            enable_power_menu: true,
        }
    }
}

/// Environment configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvironmentConfig {
    /// Additional environment variables
    pub vars: std::collections::HashMap<String, String>,
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;

        Ok(config)
    }

    /// Apply CLI argument overrides
    pub fn with_cli_overrides(mut self, args: &super::Args) -> Self {
        // Override CSS file if specified
        if let Some(ref style) = args.style {
            self.appearance.css_file = style.to_string_lossy().to_string();
        }

        // Disable accessibility if --no-accessibility
        if args.no_accessibility {
            self.accessibility.start_orca = false;
            self.accessibility.enable_audio = false;
        }

        self
    }

    /// Save the current configuration to a file
    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        std::fs::write(path, content)
            .with_context(|| format!("Failed to write config file: {:?}", path))?;

        Ok(())
    }
}

/// Persistent state between greeter sessions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub last_user: Option<String>,
    pub last_session: Option<String>,
}

impl State {
    /// Load state from file
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read state file: {:?}", path))?;

        let state: State = toml::from_str(&content)
            .with_context(|| format!("Failed to parse state file: {:?}", path))?;

        Ok(state)
    }

    /// Save state to file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create state directory: {:?}", parent))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize state")?;

        std::fs::write(path, content)
            .with_context(|| format!("Failed to write state file: {:?}", path))?;

        Ok(())
    }
}
