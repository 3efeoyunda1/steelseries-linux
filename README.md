# SteelSeries Linux

[![Made with Slint](https://raw.githubusercontent.com/slint-ui/slint/master/logo/MadeWithSlint-logo-whitebg.png)](https://slint.dev)

An unofficial, open-source Linux control utility for supported SteelSeries devices.

SteelSeries Linux provides native device configuration through a command-line interface and a graphical application, without requiring SteelSeries GG or Wine.

## Supported hardware

| Device | USB endpoints | Status |
| --- | --- | --- |
| Aerox 3 Wireless Gen 2 | `1038:1890` 2.4 GHz receiver, `1038:1892` wired USB | Supported |
| Aerox 3 Wired | `1038:1836` wired USB | Supported |

Aerox 3 Wired, Aerox 3 Wireless, and Aerox 3 Wireless Gen 2 are treated as separate devices. Protocol compatibility is not assumed between models.

Bluetooth control is currently not supported.

## Features

- **DPI Control** - Configure mouse sensitivity and DPI stages.
- **Polling Rate** - Adjust device reporting frequency.
- **Lift-off Distance** - Configure tracking distance.
- **Battery Status** - Monitor battery level and charging status.
- **Battery Notifications** - Receive configurable low and critical battery alerts.
- **Power Management** - Configure sleep timers and power-saving settings.
- **Wireless Stability** - Configure wireless stability enhancement.
- **Bluetooth Smoothing** - Configure Bluetooth movement smoothing.
- **Scroll Jump Protection** - Prevent accidental scroll jumps.
- **Multi-device Support** - Manage multiple devices, including devices of the same model.
- **Device Detection** - Automatically detect supported devices and connection changes.

Feature availability varies by device model.

For detailed feature compatibility and device-specific settings, see [Device Compatibility](#device-compatibility).

## Installation

Download the latest CLI and GUI packages from [GitHub Releases](https://github.com/3efeoyunda1/steelseries-linux/releases).

Install both packages for your distribution.

### Arch Linux / CachyOS

```bash
sudo pacman -U \
  ./steelseries-linux-1.0.0-1-x86_64.pkg.tar.zst \
  ./steelseries-linux-gui-1.0.0-1-x86_64.pkg.tar.zst
```

### Debian / Ubuntu

```bash
sudo apt install \
  ./steelseries-linux_1.0.0-1_amd64.deb \
  ./steelseries-linux-gui_1.0.0-1_amd64.deb
```

### Fedora

```bash
sudo dnf install \
  ./steelseries-linux-1.0.0-1.fc45.x86_64.rpm \
  ./steelseries-linux-gui-1.0.0-1.fc45.x86_64.rpm
```

The `steelseries-linux` package provides the CLI (`steelseriesctl`) and required udev rules. The `steelseries-linux-gui` package provides the graphical application and depends on the CLI package.

For native package building and inspection instructions, see [PACKAGING.md](PACKAGING.md).

## Device Compatibility

| Feature | Aerox 3 Wireless Gen 2 | Aerox 3 Wired |
| --- | :---: | :---: |
| DPI Control | ✅ | ✅ |
| DPI Range | 50–26,000 | 200–8,500 |
| DPI Step | 50 | 100 |
| DPI Stages | 1–5 | 1–5 |
| Active DPI Stage Selection | ✅ | ✅ |
| Independent X/Y DPI | ✅ | ❌ |
| Wired Polling Rate | Up to 1000 Hz | Up to 1000 Hz |
| Wireless Polling Rate | Up to 4000 Hz | ❌ |
| Lift-off Distance | ✅ | ❌ |
| Per-stage Lift-off Distance | ✅ | ❌ |
| Battery Status | ✅ | ❌ |
| Charging Status | ✅ | ❌ |
| Battery Notifications | ✅ | ❌ |
| Low Power Mode | ✅ | ❌ |
| Low Power Polling Rate | ✅ | ❌ |
| Automatic Low Power Mode | ✅ | ❌ |
| Sleep Timer | ✅ | ❌ |
| Wireless Stability Enhancement | ✅ | ❌ |
| Bluetooth Smoothing | ✅ | ❌ |
| Scroll Jump Protection | ✅ | ❌ |
| Persistent Settings | ✅ | ✅ |
| Acceleration / Deceleration | 🟡 | 🟡 |
| Angle Snapping | 🟡 | 🟡 |
| Angle Rotation | 🟡 | ❌ |
| RGB Lighting Control | 🟡 | 🟡 |
| RGB Brightness Control | 🟡 | 🟡 |
| RGB Animation Effects | 🟡 | 🟡 |
| Button Remapping | 🟡 | 🟡 |
| Macro Configuration | 🟡 | 🟡 |

**Legend**

- ✅ Supported by SteelSeries Linux
- 🟡 Supported by the device or its official software, but not implemented in SteelSeries Linux
- ❌ Not available on the device

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

# Get polling rates for the selected device
steelseriesctl polling get

# Set the rate on a device with one polling mode, such as Aerox 3 Wired
steelseriesctl polling set 500

# Specify the mode on a device that supports both
steelseriesctl polling set wired 1000
steelseriesctl polling set wireless 4000

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

### Device Selection

When multiple supported devices are connected, use `--device` (or `-d`) to select a specific device by its ID.

```bash
# List available devices
steelseriesctl device list

# Get device information
steelseriesctl --device <ID> device describe

# Get DPI settings
steelseriesctl --device <ID> dpi get

# Get device settings as JSON
steelseriesctl --device <ID> --json device snapshot
```

For devices without a hardware ID or USB serial number, a Custom ID is generated using the device model and physical USB port location. This ID may change when the device is connected to a different USB port.

### Shell Completion

The CLI packages install Bash, Zsh, and Fish completions automatically.
Press Tab to complete commands, options, polling rates, or connected device IDs.

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

- Additional SteelSeries mouse support
- SteelSeries headset support
- SteelSeries keyboard support
- Onboard/default lighting research
- Button mapping and remapping research

RGB animation streaming is intentionally not a current priority. Onboard/default lighting behavior may be investigated separately.

## Disclaimer

This is an unofficial community project and is not affiliated with or endorsed by SteelSeries.

SteelSeries and its product names are trademarks of their respective owners.

## License

The project's original source code is licensed under the [MIT License](LICENSE).

Third-party dependency licenses are available in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

Redistribution permission for the SteelSeries product images has not been verified.
The project MIT license excludes those images.
