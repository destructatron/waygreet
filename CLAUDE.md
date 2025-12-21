# Waygreet

Accessibility-first Wayland GTK greeter for greetd.

## Build Commands

```bash
# Check for errors (fast)
cargo check

# Build debug
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run in demo mode (no greetd required)
waygreet --demo

# Run demo without accessibility services (faster testing)
waygreet --demo --no-accessibility
```

## Architecture

### Overview

Waygreet is a GTK4 greeter using Relm4 reactive framework that runs inside cage Wayland compositor. It communicates with greetd via Unix socket IPC.

```
greetd
  └── cage -s -- waygreet
        ├── PipeWire + WirePlumber + pipewire-pulse (audio)
        ├── Orca (screen reader)
        └── GTK4 UI (login form, session selector, power menu)
```

### Key Modules

- **src/main.rs**: Entry point, CLI parsing, accessibility environment setup
- **src/app.rs**: Root Relm4 AsyncComponent with authentication state machine
- **src/greetd.rs**: greetd IPC (JSON over Unix socket with u32 length prefix)
- **src/sessions.rs**: Desktop entry parsing for session discovery
- **src/session_env.rs**: XDG_RUNTIME_DIR and D-Bus session setup
- **src/config.rs**: TOML configuration with serde
- **src/style.rs**: CSS loading with high-contrast support

### Accessibility Modules

- **src/accessibility/audio.rs**: Starts pipewire, wireplumber, pipewire-pulse
- **src/accessibility/orca.rs**: Orca screen reader management
- **src/accessibility/systemd.rs**: systemd user service helpers

### UI Components

- **src/components/login_form.rs**: Username/password entry
- **src/components/session_selector.rs**: Wayland/X11 session dropdown
- **src/components/power_menu.rs**: Reboot/shutdown buttons

## Known Patterns and Pitfalls

### GTK Entry and #[watch]

Do NOT use `#[watch]` on `set_text` for GTK Entry widgets. GTK emits a `changed` signal when `set_text` is called programmatically, which creates an infinite feedback loop:

```rust
// BAD - causes infinite loop and CPU spike
#[name = "username_entry"]
gtk4::Entry {
    #[watch]
    set_text: &model.username,  // DON'T DO THIS
}

// GOOD - only watch sensitive, handle text via connect_changed
#[name = "username_entry"]
gtk4::Entry {
    #[watch]
    set_sensitive: model.enabled,
    connect_changed[sender] => move |entry| {
        sender.input(LoginFormInput::SetUsername(entry.text().to_string()));
    },
}
```

### RelmApp CLI Arguments

GTK's Application parses argv and fails on unknown options. Pass empty args to RelmApp:

```rust
let app = RelmApp::new("org.waygreet.Greeter")
    .with_args(Vec::<String>::new());
```

### greetd IPC

The greetd protocol uses JSON messages with a u32 length prefix. The `greetd_ipc` crate's TokioCodec is a trait, not a struct - implementation uses direct socket I/O with `AsyncReadExt`/`AsyncWriteExt`.

### Accessibility Setup Requirements

Orca and audio work out-of-the-box with no special configuration required:
- Portal services are disabled via environment variables (`GDK_DEBUG=no-portals`, etc.)
- User lingering and manual service enablement are NOT required
- The greetd PAM session automatically creates a systemd user instance for the greeter user
- PipeWire services start automatically when enabled

### Orca Startup Timing

Orca MUST start AFTER GTK initializes. The AT-SPI bus (accessibility bus) is created by GTK when the window is realized. Starting Orca before GTK results in:
```
Cannot start the screen reader because it cannot connect to the Desktop.
AT-SPI: Error in GetItems, sender=:1.0, error=Unknown object '/org/a11y/atspi/cache'
```

The fix is to use `glib::idle_add_local_once()` to schedule Orca startup after the main loop begins.

### Orca Environment Variables

Orca must be started directly as a subprocess (not via systemd `orca.service`) so it inherits waygreet's environment:
- `WAYLAND_DISPLAY` - Required for Wayland access
- `AT_SPI2_*` - AT-SPI bus addresses
- `DBUS_SESSION_BUS_ADDRESS` - D-Bus session bus

The systemd orca.service doesn't have these set correctly.

### Portal Services and Startup Delays

GTK tries to activate `xdg-desktop-portal` on startup via D-Bus, which times out after 25+ seconds when unavailable. This is a known issue with GTK greeters running in cage (documented in cage issue #169).

**Solution: Use `dbus-run-session` wrapper in greetd config:**
```toml
command = "dbus-run-session cage -s -- waygreet"
```

This creates a fresh D-Bus session for waygreet where our portal prevention measures take effect:

1. **Process environment** (set in `main.rs`):
   - `GDK_DEBUG=no-portals` - Disables all portal usage at the GDK level
   - `GTK_USE_PORTAL=0` - Disables portal file dialogs
   - `GIO_USE_VFS=local` - Disables gvfs mount discovery
   - `GSETTINGS_BACKEND=memory` - Uses in-memory settings instead of dconf

2. **D-Bus service file overrides** (created in `session_env.rs`):
   - Creates fake `.service` files in `$XDG_RUNTIME_DIR/waygreet/dbus-1/services/`
   - Each override has `Exec=/bin/false` which makes activation fail immediately
   - Prepends this directory to `XDG_DATA_DIRS` so D-Bus finds overrides first
   - This prevents the 25-second timeout by making activation fail fast

3. **D-Bus activation environment** (set in `session_env.rs`):
   - Uses `dbus-update-activation-environment --systemd` to propagate settings
   - Unsets `XDG_CURRENT_DESKTOP` which triggers portal backend lookups

No external configuration (like masking services) is required.

### Audio for Orca

Orca requires PulseAudio compatibility. All three services must be running:
1. pipewire.socket (triggers pipewire.service)
2. wireplumber.service
3. pipewire-pulse.socket (provides PulseAudio compatibility)

### Desktop Entry Parsing

`freedesktop_entry_parser::parse_entry()` takes a Path, not file contents:

```rust
let entry = parse_entry(path)?;  // path: &Path
```

## Configuration

Default config location: `/etc/greetd/waygreet.toml`

See `data/waygreet.toml.example` for all options.

## Testing

```bash
# Run without greetd connection
waygreet --demo

# Run without accessibility services (faster)
waygreet --demo --no-accessibility

# With debug logging
waygreet --demo --log-level debug
```
