# Waygreet

An accessibility-first Wayland GTK greeter for greetd.

## Features

- **Accessibility First**: Starts Orca screen reader and PipeWire audio by default
- **Wayland Native**: Runs inside cage (or other Wayland compositors)
- **GTK4 + Relm4**: Modern Rust GUI with reactive architecture
- **systemd Integration**: Uses systemd to manage Orca and PipeWire when available
- **Session Discovery**: Automatically finds Wayland and X11 sessions
- **Customizable**: CSS theming with high-contrast mode support

## Installation

### Dependencies

- GTK4
- libadwaita
- gtk4-layer-shell
- greetd
- cage (recommended Wayland compositor)
- Orca (screen reader)
- PipeWire (audio)

### Building

```bash
cargo build --release
sudo install -Dm755 target/release/waygreet /usr/bin/waygreet
```

### Configuration

1. Create a dedicated greeter user (if not already created by greetd package):
```bash
sudo useradd -r -s /sbin/nologin -d /var/lib/greetd greetd
```

2. Mask portal services (prevents 25+ second startup delays):
```bash
sudo machinectl shell greetd@ /bin/bash -c "systemctl --user mask xdg-desktop-portal.service xdg-desktop-portal-gtk.service xdg-desktop-portal-gnome.service gvfs-daemon.service"
```

3. Configure greetd (`/etc/greetd/config.toml`):
```toml
[terminal]
vt = 1

[default_session]
command = "cage -s -- waygreet"
user = "greetd"
```

4. Optional: Create waygreet config (`/etc/greetd/waygreet.toml`):
```toml
[accessibility]
start_orca = true
enable_audio = true

[appearance]
high_contrast = false
font_scale = 1.0
```

Note: PipeWire audio services are started automatically by systemd when the greetd session begins. No manual service enablement or user lingering is required.

## Usage

Waygreet is launched by greetd. It provides:

- Username and password entry
- Session selection (Wayland/X11)
- Power menu (reboot/shutdown)

### Keyboard Shortcuts

- **Tab/Shift+Tab**: Navigate between fields
- **Enter**: Submit login or activate button
- **Arrow keys**: Navigate session dropdown

### Command Line Options

```
waygreet [OPTIONS]

Options:
  -c, --config <FILE>     Config file path [default: /etc/greetd/waygreet.toml]
  -s, --style <FILE>      Custom CSS stylesheet
      --demo              Run in demo mode (no greetd)
      --no-accessibility  Skip starting accessibility services
      --log-level <LEVEL> Log level [default: info]
  -h, --help              Print help
  -V, --version           Print version
```

## Accessibility

Waygreet is designed for users who rely on screen readers:

1. **Orca starts automatically** after the UI loads (required for AT-SPI bus)
2. **PipeWire audio** runs via systemd user services (requires lingering)
3. **AT-SPI environment** is configured for accessibility
4. **High contrast mode** available in config
5. **Font scaling** configurable
6. All UI elements have accessible names

### Orca Integration

Orca is started directly as a subprocess of waygreet (not via systemd) so it inherits the correct environment variables (`WAYLAND_DISPLAY`, AT-SPI bus addresses). This ensures Orca can connect to the accessibility bus created by GTK.

## Theming

Custom CSS can be provided via config or command line:

```toml
[appearance]
css_file = "/etc/greetd/waygreet.css"
high_contrast = false
font_scale = 1.5
```

See `data/waygreet.css` for the default theme and `data/waygreet-high-contrast.css` for high contrast.

## Troubleshooting

### Greeter takes a long time to appear

Portal services are likely trying to start and timing out. Mask them:
```bash
sudo machinectl shell greetd@ /bin/bash -c "systemctl --user mask xdg-desktop-portal.service xdg-desktop-portal-gtk.service xdg-desktop-portal-gnome.service"
```

### Orca not speaking

1. **Check PipeWire is running for greetd user:**
   ```bash
   sudo machinectl shell greetd@ /bin/bash -c "systemctl --user status pipewire pipewire-pulse wireplumber"
   ```

2. **Check Orca process:**
   ```bash
   ps aux | grep orca
   ```

3. **Check logs for errors:**
   ```bash
   journalctl -b | grep -iE "(orca|at-spi|pipewire)" | tail -30
   ```

### "Cannot start the screen reader because it cannot connect to the Desktop"

This error means Orca started before GTK initialized the AT-SPI bus. This should not happen with the current code, but if it does, check that you're running the latest version.

### Session not starting

1. Check greetd logs: `journalctl -u greetd`
2. Verify session files exist in `/usr/share/wayland-sessions/` or `/usr/share/xsessions/`

### Demo mode

Test without greetd:
```bash
waygreet --demo

# Without accessibility (faster for UI testing)
waygreet --demo --no-accessibility

# With debug logging
waygreet --demo --log-level debug
```

## License

MIT
