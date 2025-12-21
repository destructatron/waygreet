# Waygreet systemd Services

This directory contains systemd user service files for accessibility support.

## Installation

Copy the service files to the greeter user's systemd directory:

```bash
# For the greeter user
sudo mkdir -p /var/lib/greeter/.config/systemd/user
sudo cp waygreet-orca.service /var/lib/greeter/.config/systemd/user/

# Or install system-wide for all users
sudo cp waygreet-orca.service /usr/lib/systemd/user/
```

## Services

### waygreet-orca.service

Starts the Orca screen reader for the greeter session. This service:

- Starts after the AT-SPI D-Bus bus and PipeWire
- Uses `orca --replace` to replace any existing Orca instance
- Sets necessary environment variables for accessibility
- Uses systemd's watchdog for automatic restart on failure

## Notes

The greeter user needs:

1. A home directory with proper permissions
2. Access to `/run/user/<uid>` for the runtime directory
3. D-Bus session bus access

Typically, greetd handles setting up the environment for the greeter user.
