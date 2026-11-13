# SteelSeries Linux

Unofficial Linux control utility for supported SteelSeries devices.

> This project is in early development and is not affiliated with or endorsed by SteelSeries.

## Supported hardware

| Device | USB endpoints | Status |
| --- | --- | --- |
| Aerox 3 Wireless Gen 2 | `1038:1890` 2.4 GHz receiver, `1038:1892` wired USB | Supported |

Aerox 3, Aerox 3 Wireless, and Aerox 3 Wireless Gen 2 are treated as separate devices. Protocol compatibility is not assumed between models.

Bluetooth control is currently not supported.

## Features

- **DPI Control** - Adjust mouse sensitivity, configure DPI stages, and customize X/Y axes independently.
- **Polling Rate** - Configure how frequently the mouse reports its position.
- **Lift-off Distance** - Adjust the height at which the mouse stops tracking.
- **Battery Status** - Monitor battery level and charging status from the system tray.
- **Power Management** - Configure sleep timers and automatic power-saving settings.
- **Wireless Stability** - Enable or disable wireless stability enhancement.
- **Bluetooth Smoothing** - Toggle Bluetooth movement smoothing.
- **Scroll Jump Protection** - Prevent accidental scroll jumps with an adjustable delay.
- **Device Detection** - Automatically detect supported mice and connection changes.
- **Battery Notifications (Coming Soon)** - Get notified when the battery is low or critically low.

## Installation

Download the latest CLI and GUI packages from [GitHub Releases](https://github.com/3efeoyunda1/steelseries-linux/releases).

Install both packages for your distribution.

### Arch Linux / CachyOS

```bash
sudo pacman -U \
  ./steelseries-linux-0.4.1-1-x86_64.pkg.tar.zst \
  ./steelseries-linux-gui-0.4.1-1-x86_64.pkg.tar.zst
```

### Debian / Ubuntu

```bash
sudo apt install \
  ./steelseries-linux_0.4.1-1_amd64.deb \
  ./steelseries-linux-gui_0.4.1-1_amd64.deb
```

### Fedora

```bash
sudo dnf install \
  ./steelseries-linux-0.4.1-1.fc45.x86_64 \
  ./steelseries-linux-gui-0.4.1-1.fc45.x86_64
```

The `steelseries-linux` package provides the CLI (`steelseriesctl`) and required udev rules. The `steelseries-linux-gui` package provides the graphical application and depends on the CLI package.

For native package building and inspection instructions, see [PACKAGING.md](PACKAGING.md).

## Usage

```bash
# Show available commands
steelseriesctl --help

# List connected devices
steelseriesctl device list

# Show device information and supported features
steelseriesctl device describe

# Get current DPI settings
steelseriesctl dpi get

# Configure DPI stages
steelseriesctl dpi set 400 800 1600

# Switch to a DPI stage
steelseriesctl dpi use 2

# Get and configure polling rate
steelseriesctl polling get
steelseriesctl polling set wireless 1000

# Get and configure lift-off distance
steelseriesctl lod get
steelseriesctl lod set low 1

# Check battery level and charging status
steelseriesctl battery get

# View power settings
steelseriesctl power get

# Set sleep timer (minutes)
steelseriesctl power sleep set 30

# Configure wireless stability
steelseriesctl wireless-stability set on

# Configure Bluetooth smoothing
steelseriesctl bluetooth-smoothing set on

# Configure scroll jump protection
steelseriesctl scroll-jump set on
```

### Output Formats

For scripting and integration with other tools:

```bash
# JSON output
steelseriesctl --json device list

# Get all device settings as JSON
steelseriesctl --json device snapshot

# Get battery information as JSON
steelseriesctl --json battery get
```

## Roadmap

### v0.5

- Battery notifications
- Improve recovery when USB/2.4 GHz endpoints change or a device reconnects.
- Aerox 3 Wired support (Probably)

### Later

- Onboard/default lighting research
- Additional SteelSeries devices
- Button mapping / remapping research

RGB animation streaming is intentionally not a current priority; onboard/default lighting behavior may be investigated separately.

## Disclaimer

This is an unofficial community project and is not affiliated with or endorsed by SteelSeries.

SteelSeries and its product names are trademarks of their respective owners.

## License

MIT
