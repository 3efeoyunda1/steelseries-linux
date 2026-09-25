# SteelSeries Linux

Unofficial Linux control utility for supported SteelSeries devices.

Currently supported:

- SteelSeries Aerox 3 Wireless Gen 2
  - `1038:1890` — 2.4 GHz receiver
  - `1038:1892` — wired USB

> This project is in early development.

## Features

### Aerox 3 Wireless Gen 2

* Device detection
* Read DPI stages
* Configure 1–5 DPI stages
* Select active DPI stage
* Read polling rates
* Configure 2.4 GHz wireless polling rate
* Configure wired polling rate
* Read battery and charging status
* Automatic wired USB / linked 2.4 GHz receiver selection
* Physical-device deduplication across USB endpoints
* Persistent device configuration

Supported polling rates:

| Connection       | Supported rates                    |
| ---------------- | ---------------------------------- |
| 2.4 GHz Wireless | 125, 250, 500, 1000, 2000, 4000 Hz |
| Wired            | 125, 250, 500, 1000 Hz             |

The CLI identifies physical mice using the device identity returned by the mouse. When the same
mouse is visible through both wired USB and its linked 2.4 GHz receiver, it is listed once and the
wired endpoint is preferred automatically. A receiver without an active 2.4 GHz mouse link is not
used for control. Bluetooth control is not supported.

---

## Installation

Native packages install `steelseriesctl` and its udev permissions automatically. Package users do
not need to run the source-tree udev installation script. After installation, reconnect the mouse
or receiver if its desktop-session ACL does not update immediately.

### Arch Linux / CachyOS

```bash
sudo pacman -U ./steelseries-linux-0.2.0-1-x86_64.pkg.tar.zst
```

### Debian / Ubuntu

```bash
sudo apt install ./steelseries-linux_0.2.0-1_amd64.deb
```

### Fedora

```bash
sudo dnf install ./steelseries-linux-0.2.0-1*.x86_64.rpm
```

Package build instructions and content-inspection commands are documented in
[`PACKAGING.md`](PACKAGING.md).

### Build and run from source

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

This installs `udev/70-steelseries-linux.rules` as
`/usr/lib/udev/rules.d/70-steelseries-linux.rules` and reloads udev. Reconnect already attached
mouse and receiver endpoints if necessary. Run `steelseriesctl` itself as the normal desktop user,
not with `sudo`.

The binary will be created at:

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

After that:

```bash
steelseriesctl devices
```

---

## Usage

### Detect devices

```bash
steelseriesctl devices
```

### Select a physical device

The device ID shown by `steelseriesctl devices` can select a specific physical mouse:

```bash
steelseriesctl --device 6271700431492500250 dpi get
```

`--device` (or `-d`) is optional when exactly one usable supported mouse is connected and required
when multiple supported physical mice are connected. The ID remains the same when a mouse switches
between wired USB and its linked 2.4 GHz receiver because both endpoints are grouped by device
identity.

### DPI

Show DPI commands:

```bash
steelseriesctl dpi
```

Read current DPI configuration:

```bash
steelseriesctl dpi get
```

Set DPI stages:

```bash
steelseriesctl dpi set 400 800 1600
# Up to 5 stages are supported.
```

Select an existing DPI stage:

```bash
steelseriesctl dpi use 800
```

Example:

```text
DPI Stages:
  1: 400 DPI
> 2: 800 DPI
  3: 1600 DPI

Active: 800 DPI
```

### Polling Rate

Show polling commands:

```bash
steelseriesctl polling
```

Read current polling rates:

```bash
steelseriesctl polling get
```

Set 2.4 GHz wireless polling rate:

```bash
steelseriesctl polling set wireless 4000
```

Set wired polling rate:

```bash
steelseriesctl polling set wired 1000
```

### Battery

Show battery commands:

```bash
steelseriesctl battery
```

Read battery and charging status:

```bash
steelseriesctl battery get
```

Example:

```text
Battery: 20%
Charging: No
```

---

## Supported Devices

| Device                 | USB endpoints                         | Support                 |
| ---------------------- | ------------------------------------- | ----------------------- |
| Aerox 3 Wireless Gen 2 | `1038:1890` receiver, `1038:1892` USB | DPI + Polling + Battery |

Aerox 3, Aerox 3 Wireless and Aerox 3 Wireless Gen 2 are treated as separate devices. Protocol compatibility is not assumed between models.

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

Format:

```bash
cargo fmt
```

Project structure:

```text
crates/
├── steelseries-core/
└── steelseries-cli/
```

`steelseries-core` contains device discovery and protocol handling.

`steelseries-cli` provides the `steelseriesctl` command.

---

## Planned

* RGB control
* Sleep timer
* Lift-off distance
* Button mapping
* More SteelSeries devices
* Arch Linux / AUR package
* Separate graphical UI and tray application

---

## Disclaimer

This is an unofficial community project and is not affiliated with or endorsed by SteelSeries.

SteelSeries and its product names are trademarks of their respective owners.

## License

MIT
