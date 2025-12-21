//! Session environment setup
//!
//! Handles creation of XDG_RUNTIME_DIR and D-Bus session bus if not already available.

use anyhow::{Context, Result};
use nix::unistd::getuid;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Portal service names that need to be disabled to prevent 25+ second timeouts
const PORTAL_SERVICES: &[&str] = &[
    "org.freedesktop.portal.Desktop",
    "org.freedesktop.impl.portal.desktop.gnome",
    "org.freedesktop.impl.portal.desktop.gtk",
    "org.freedesktop.impl.portal.desktop.kde",
    "org.freedesktop.impl.portal.desktop.wlr",
    "org.freedesktop.impl.portal.desktop.hyprland",
    "org.freedesktop.impl.portal.desktop.cosmic",
];

/// Ensure the session environment is properly set up
pub async fn ensure_session_environment() -> Result<()> {
    // Check and create XDG_RUNTIME_DIR if needed
    ensure_xdg_runtime_dir()?;

    // Create D-Bus service overrides to prevent portal activation
    // This MUST happen before D-Bus session is set up
    create_portal_service_overrides()?;

    // Check and start D-Bus session if needed
    ensure_dbus_session().await?;

    // Update D-Bus activation environment to prevent portal activation
    update_dbus_activation_environment().await;

    // Check systemd user session availability
    check_systemd_user_session().await;

    Ok(())
}

/// Create D-Bus service file overrides to prevent portal activation
///
/// D-Bus looks for service files in `<XDG_DATA_DIRS>/dbus-1/services/`.
/// By creating override files with `Exec=/bin/false`, we make D-Bus activation
/// fail immediately instead of timing out after 25+ seconds.
fn create_portal_service_overrides() -> Result<()> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", getuid()));

    // Create directory structure: <runtime>/waygreet/dbus-1/services/
    let base_dir = PathBuf::from(&runtime_dir).join("waygreet");
    let services_dir = base_dir.join("dbus-1").join("services");
    std::fs::create_dir_all(&services_dir)
        .context("Failed to create D-Bus services override directory")?;

    // Create override service files that immediately fail
    for service_name in PORTAL_SERVICES {
        let service_file = services_dir.join(format!("{}.service", service_name));
        let content = format!(
            "[D-BUS Service]\nName={}\nExec=/bin/false\n",
            service_name
        );
        if let Err(e) = std::fs::write(&service_file, &content) {
            warn!("Failed to create service override {:?}: {}", service_file, e);
        }
    }

    // Prepend our base directory to XDG_DATA_DIRS so D-Bus finds our overrides first
    let current_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let new_data_dirs = format!("{}:{}", base_dir.display(), current_data_dirs);
    std::env::set_var("XDG_DATA_DIRS", &new_data_dirs);

    info!("Created D-Bus portal service overrides in {:?}", services_dir);
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

/// Update D-Bus activation environment to prevent portal service activation
///
/// When D-Bus tries to activate services (like xdg-desktop-portal), it uses its
/// own "activation environment" which is separate from our process environment.
/// We need to update that environment to prevent 25+ second timeouts from portal
/// services that aren't available in the greeter context.
async fn update_dbus_activation_environment() {
    // First, unset XDG_CURRENT_DESKTOP which triggers portal backend lookups
    let unset_result = Command::new("dbus-update-activation-environment")
        .arg("--systemd")
        .arg("--unset=XDG_CURRENT_DESKTOP")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match unset_result {
        Ok(status) if status.success() => {
            debug!("Unset XDG_CURRENT_DESKTOP from D-Bus activation environment");
        }
        Ok(_) => {
            debug!("dbus-update-activation-environment --unset failed (non-fatal)");
        }
        Err(e) => {
            debug!("Could not run dbus-update-activation-environment: {} (non-fatal)", e);
            return;
        }
    }

    // Set portal-disabling variables in the D-Bus activation environment
    let set_result = Command::new("dbus-update-activation-environment")
        .arg("--systemd")
        .args([
            "GDK_DEBUG=no-portals",
            "GTK_USE_PORTAL=0",
            "GIO_USE_VFS=local",
            "GSETTINGS_BACKEND=memory",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await;

    match set_result {
        Ok(status) if status.success() => {
            info!("Updated D-Bus activation environment to disable portals");
        }
        Ok(_) => {
            warn!("Failed to update D-Bus activation environment");
        }
        Err(e) => {
            warn!("Could not update D-Bus activation environment: {}", e);
        }
    }
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
