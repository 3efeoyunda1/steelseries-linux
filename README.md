# SteelSeries Linux

Unofficial Linux control utility for supported SteelSeries devices.

Currently supported:
- SteelSeries Aerox 3 Wireless Gen 2 (`1038:1890`)

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
* Persistent device configuration

Supported polling rates:

| Connection       | Supported rates                    |
| ---------------- | ---------------------------------- |
| 2.4 GHz Wireless | 125, 250, 500, 1000, 2000, 4000 Hz |
| Wired            | 125, 250, 500, 1000 Hz             |

Bluetooth configuration is not implemented yet.

---

## Installation

### Arch Linux / CachyOS

A native Arch package will be provided in GitHub Releases.

After downloading the package:

```bash
sudo pacman -U steelseries-linux-0.1.0-1-x86_64.pkg.tar.zst
```

Then:

```bash
steelseriesctl devices
```

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

---

## Supported Devices

| Device                 |    VID |    PID | Support       |
| ---------------------- | -----: | -----: | ------------- |
| Aerox 3 Wireless Gen 2 | `1038` | `1890` | DPI + Polling |

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

* Battery status
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
