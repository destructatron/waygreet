//! Session environment setup
//!
//! Handles creation of XDG_RUNTIME_DIR and D-Bus session bus if not already available.

use anyhow::{Context, Result};
use nix::unistd::getuid;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Ensure the session environment is properly set up
pub async fn ensure_session_environment() -> Result<()> {
    // Check and create XDG_RUNTIME_DIR if needed
    ensure_xdg_runtime_dir()?;

    // Check and start D-Bus session if needed
    ensure_dbus_session().await?;

    // Check systemd user session availability
    check_systemd_user_session().await;

    Ok(())
}

/// Ensure XDG_RUNTIME_DIR exists and is properly set
fn ensure_xdg_runtime_dir() -> Result<()> {
    let uid = getuid();

    // Check if already set
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(&dir);
        if path.exists() {
            debug!("XDG_RUNTIME_DIR already set to {:?}", path);
            return Ok(());
        }
        warn!("XDG_RUNTIME_DIR set to {:?} but doesn't exist", path);
    }

    // Default location
    let runtime_dir = PathBuf::from(format!("/run/user/{}", uid));

    if runtime_dir.exists() {
        std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);
        info!("Set XDG_RUNTIME_DIR to {:?}", runtime_dir);
        return Ok(());
    }

    // Try to create it (requires appropriate permissions)
    info!("Attempting to create {:?}", runtime_dir);

    match std::fs::create_dir_all(&runtime_dir) {
        Ok(()) => {
            // Set proper permissions (0700)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                std::fs::set_permissions(&runtime_dir, perms)
                    .context("Failed to set XDG_RUNTIME_DIR permissions")?;
            }

            std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);
            info!("Created and set XDG_RUNTIME_DIR to {:?}", runtime_dir);
            Ok(())
        }
        Err(e) => {
            // Fall back to /tmp if we can't create in /run/user
            warn!("Failed to create {:?}: {}", runtime_dir, e);

            let fallback = PathBuf::from(format!("/tmp/waygreet-runtime-{}", uid));
            std::fs::create_dir_all(&fallback)
                .context("Failed to create fallback runtime directory")?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = std::fs::Permissions::from_mode(0o700);
                std::fs::set_permissions(&fallback, perms)?;
            }

            std::env::set_var("XDG_RUNTIME_DIR", &fallback);
            warn!("Using fallback XDG_RUNTIME_DIR: {:?}", fallback);
            Ok(())
        }
    }
}

/// Ensure D-Bus session bus is available
async fn ensure_dbus_session() -> Result<()> {
    // Check if already set
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_ok() {
        debug!("DBUS_SESSION_BUS_ADDRESS already set");
        return Ok(());
    }

    // Try to get address from the runtime directory
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", getuid()));

    let bus_path = PathBuf::from(&runtime_dir).join("bus");
    if bus_path.exists() {
        let address = format!("unix:path={}", bus_path.display());
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &address);
        info!("Set DBUS_SESSION_BUS_ADDRESS to {}", address);
        return Ok(());
    }

    // Try to start dbus-daemon
    info!("Starting D-Bus session daemon");

    let output = Command::new("dbus-daemon")
        .args(["--session", "--print-address", "--fork"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to start dbus-daemon")?;

    if output.status.success() {
        let address = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !address.is_empty() {
            std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &address);
            info!("Started D-Bus session at {}", address);
            return Ok(());
        }
    }

    // If that fails, try dbus-launch
    let output = Command::new("dbus-launch")
        .args(["--sh-syntax"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("Failed to run dbus-launch")?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("DBUS_SESSION_BUS_ADDRESS=") {
                let address = line
                    .strip_prefix("DBUS_SESSION_BUS_ADDRESS=")
                    .unwrap_or("")
                    .trim_matches('\'')
                    .trim_matches('"')
                    .trim_end_matches(';');

                std::env::set_var("DBUS_SESSION_BUS_ADDRESS", address);
                info!("Started D-Bus session via dbus-launch at {}", address);
                return Ok(());
            }
        }
    }

    warn!("Could not start D-Bus session - some features may not work");
    Ok(())
}

/// Check if systemd user session is available
async fn check_systemd_user_session() {
    // Try to connect to the user's systemd instance
    let output = Command::new("systemctl")
        .args(["--user", "is-system-running"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            info!("systemd user session available (status: {})", status);
        }
        Ok(output) => {
            let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
            debug!("systemd user session status: {}", status);
        }
        Err(e) => {
            warn!("Could not check systemd user session: {}", e);
        }
    }
}

/// Get the UID of the current user
#[allow(dead_code)]
pub fn get_current_uid() -> u32 {
    getuid().as_raw()
}
