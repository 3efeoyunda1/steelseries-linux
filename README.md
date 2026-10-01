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

### Aerox 3 Wireless Gen 2

- Physical-device detection and deduplication
- Automatic wired USB / linked 2.4 GHz receiver selection
- Stable device identity across wired and 2.4 GHz connections
- Read and configure 1–5 DPI stages
- Independent X/Y DPI values
- Select the active DPI stage
- Read and configure per-stage lift-off distance (LOD)
- Read and configure wired and 2.4 GHz polling rates
- Read battery percentage and charging state
- Power-management controls
  - Low Power Mode
  - Low Power polling rate
  - Sleep Timer
  - Auto Low Power
  - Auto Low Power battery threshold
- Wireless Stability Enhancement
- Bluetooth Smoothing
- Scroll Jump Protection with configurable delay
- Persistent configuration writes
- Machine-readable JSON output for scripts and frontends

### Supported polling rates

| Connection | Supported rates |
| --- | --- |
| 2.4 GHz wireless | 125, 250, 500, 1000, 2000, 4000 Hz |
| Wired | 125, 250, 500, 1000 Hz |

The CLI identifies physical mice using the device identity returned by the mouse. When the same mouse is visible through both wired USB and its linked 2.4 GHz receiver, it is listed once and the wired endpoint is preferred automatically. A receiver without an active 2.4 GHz mouse link is not used for control.

---

## Installation

Native packages install `steelseriesctl` and the required udev permissions automatically. Package users do not need to run the source-tree udev installation script.

After installation, reconnect the mouse or receiver if its desktop-session ACL does not update immediately.

### Arch Linux / CachyOS

```bash
sudo pacman -U ./steelseries-linux-0.3.0-1-x86_64.pkg.tar.zst
```

### Debian / Ubuntu

```bash
sudo apt install ./steelseries-linux_0.3.0-1_amd64.deb
```

### Fedora

```bash
sudo dnf install ./steelseries-linux-0.3.0-1*.x86_64.rpm
```

Package build instructions and package-content inspection commands are documented in [`PACKAGING.md`](PACKAGING.md).

### Build from source

Install the required packages:

```bash
sudo pacman -S --needed base-devel rustup pkgconf hidapi libusb
rustup default stable
```

Clone and build:

```bash
git clone https://github.com/3efeoyunda1/steelseries-linux.git
cd steelseries-linux

cargo build --release --locked
```

Source/development users must install the included systemd-logind/uaccess rules separately:

```bash
sudo ./scripts/install-udev-rules.sh
```

This installs `udev/70-steelseries-linux.rules` as `/usr/lib/udev/rules.d/70-steelseries-linux.rules` and reloads udev.

Reconnect already attached mouse and receiver endpoints if necessary. Run `steelseriesctl` as the normal desktop user, not with `sudo`.

The binary is created at:

```text
target/release/steelseriesctl
```

Run it directly:

```bash
./target/release/steelseriesctl devices
```

Or install it manually:

```bash
sudo install -Dm755 target/release/steelseriesctl /usr/local/bin/steelseriesctl
```

---

## Usage

### Detect devices

```bash
steelseriesctl devices
```

Example:

```text
SteelSeries devices:
  Aerox 3 Wireless Gen 2
    ID: 6271700431492500250
    Connection: 2.4 GHz
```

### Select a physical device

Use the device ID shown by `steelseriesctl devices`:

```bash
steelseriesctl --device 6271700431492500250 dpi get
```

`--device` (or `-d`) is optional when exactly one usable supported mouse is connected and required when multiple supported physical mice are connected.

The ID remains the same when a mouse switches between wired USB and its linked 2.4 GHz receiver because both endpoints are grouped by device identity.

---

## JSON output

Human-readable output remains the default. Use the global `--json` flag for the stable machine-readable interface intended for scripts, frontends, and the future graphical application:

```bash
steelseriesctl --json devices
steelseriesctl --json dpi get
steelseriesctl --json lod get
steelseriesctl --json polling get
steelseriesctl --json battery get
steelseriesctl --json power get
steelseriesctl --json wireless-stability get
steelseriesctl --json bluetooth-smoothing get
steelseriesctl --json scroll-jump get
```

Example:

```json
{"schema_version":1,"ok":true,"command":"scroll-jump.get","data":{"enabled":true,"delay_ms":500}}
```

The JSON schema version is currently `1`.

Successful commands write exactly one JSON document to stdout. Runtime failures write one JSON error document to stderr and return a non-zero exit status.

---

## DPI

Read the current DPI configuration:

```bash
steelseriesctl dpi get
```

Set DPI stages:

```bash
steelseriesctl dpi set 400 800 1600
steelseriesctl dpi set 400 800x1600 3200
```

Up to 5 stages are supported.

A single value applies to both X and Y axes. Use `XxY`, for example `800x1600`, to configure the axes independently. Uppercase `X` is also accepted as input.

Aerox 3 Wireless Gen 2 values must be between 50 and 26000 DPI in steps of 50 on each axis.

Select an existing DPI stage:

```bash
steelseriesctl dpi use 2
```

`dpi use` selects by the one-based stage ID shown by `dpi get`, not by DPI value.

---

## Lift-off Distance

Read lift-off distance for every configured DPI stage:

```bash
steelseriesctl lod get
```

Set one stage to Low (1 mm) or High (2 mm):

```bash
steelseriesctl lod set low 1
steelseriesctl lod set high 4
```

---

## Polling Rate

Read current polling rates:

```bash
steelseriesctl polling get
```

Set the 2.4 GHz wireless polling rate:

```bash
steelseriesctl polling set wireless 4000
```

Set the wired polling rate:

```bash
steelseriesctl polling set wired 1000
```

---

## Battery

Read battery and charging status:

```bash
steelseriesctl battery get
```

Battery telemetry may temporarily be unavailable after switching connection modes.

---

## Power Management

Read the complete power configuration:

```bash
steelseriesctl power get
```

Change individual settings while preserving the other power fields:

```bash
steelseriesctl power low-power set on
steelseriesctl power low-power set off
steelseriesctl power low-power polling 125

steelseriesctl power auto-low-power set on
steelseriesctl power auto-low-power set off
steelseriesctl power auto-low-power threshold 10

steelseriesctl power sleep set 30
```

Low Power polling accepts 125, 250, or 500 Hz.

Auto Low Power thresholds accept 5–25%.

The sleep timer accepts whole minutes from 1 through 71582. The upper value is the largest whole-minute value encodable by the verified `u32` millisecond field; hardware testing currently covers values through 1440 minutes.

---

## Wireless Stability Enhancement

```bash
steelseriesctl wireless-stability
steelseriesctl wireless-stability get
steelseriesctl wireless-stability set on
steelseriesctl wireless-stability set off
```

## Bluetooth Smoothing

```bash
steelseriesctl bluetooth-smoothing
steelseriesctl bluetooth-smoothing get
steelseriesctl bluetooth-smoothing set on
steelseriesctl bluetooth-smoothing set off
```

Wireless Stability Enhancement and Bluetooth Smoothing are stored independently from the normal wired and 2.4 GHz polling configuration. Changing one toggle preserves the other.

---

## Scroll Jump Protection

```bash
steelseriesctl scroll-jump
steelseriesctl scroll-jump get
steelseriesctl scroll-jump set on
steelseriesctl scroll-jump delay 500
steelseriesctl scroll-jump set off
```

The delay is configured in milliseconds. The currently supported CLI range is 100–1500 ms in 100 ms steps.

Changing the delay preserves the enabled state, and changing the enabled state preserves the delay.

---

## Architecture

The project is split into two Rust crates:

```text
crates/
├── steelseries-core/
└── steelseries-cli/
```

`steelseries-core` contains device discovery, capability handling, HID protocol logic, and physical-device selection.

`steelseries-cli` provides the `steelseriesctl` command and both human-readable and JSON output.

The planned graphical application is a separate executable/process and will use the `steelseriesctl --json` interface rather than requiring a background daemon.

---

## Development

Run tests:

```bash
cargo test --workspace
```

Run Clippy:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Check formatting:

```bash
cargo fmt --all -- --check
```

Check patch whitespace:

```bash
git diff --check
```

---

## Roadmap

### v0.4

- Device information command/API
- First separate graphical application
- Device overview and configuration UI
- Tray battery indicator
- GUI integration through the versioned `steelseriesctl --json` interface

### v0.5

- Aerox 3 Wired support (`1038:1836`)
- Device-specific capability handling for the wired model

### Later

- Onboard/default lighting research
- Additional SteelSeries devices
- Button mapping / remapping research
- Packaging and AUR improvements

RGB animation streaming is intentionally not a current priority; onboard/default lighting behavior may be investigated separately.

---

## Disclaimer

This is an unofficial community project and is not affiliated with or endorsed by SteelSeries.

SteelSeries and its product names are trademarks of their respective owners.

## License

MIT
