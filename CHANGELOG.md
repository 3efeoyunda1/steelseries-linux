# Changelog

## 0.4.0

### Added

- Device capability metadata for frontend and script integration
- `device describe` for model identity and supported constraints
- `device snapshot` for complete read-only state in one CLI process
- Structured JSON validation fields and details
- Initial optional Slint GUI with asynchronous device discovery, refresh, and device selection
- JSON-only CLI subprocess backend for the GUI, with `STEELSERIESCTL_PATH` development override
- Capability-aware read-only GUI dashboard using `device describe` and `device snapshot`
- Capability-driven GUI controls for DPI, LOD, polling, power, wireless features, and Scroll Jump

### Changed

- Replaced the top-level `devices` command with `device list`

## 0.3.0

### Added

- Per-stage lift-off distance (LOD) read/write support
- Independent X/Y DPI values for individual DPI stages
- Active DPI stage selection
- Power-management controls:
  - Low Power Mode
  - Low Power polling rate
  - Sleep Timer
  - Auto Low Power
  - Auto Low Power battery threshold
- Wireless Stability Enhancement controls
- Bluetooth Smoothing controls
- Scroll Jump Protection controls with configurable delay
- Global `--json` machine-readable output
- JSON schema version 1 with structured success and error responses

### Changed

- `dpi use` now selects a one-based stage ID instead of searching by DPI value
- Aerox 3 Wireless Gen 2 DPI validation now uses the device-specific 50–26000 DPI range in 50-DPI steps

## 0.2.0

### Added

- Wired USB support for Aerox 3 Wireless Gen 2 (`1038:1892`)
- Battery and charging status
- Physical device identity matching
- Automatic wired / 2.4 GHz endpoint selection
- `--device <ID>` physical-device selection
- Multi-device selection infrastructure
- Project-owned udev permissions
- Native Arch Linux package
- Native Debian/Ubuntu package
- Native Fedora package

### Improved

- HID response filtering
- Physical-device deduplication
- Receiver link validation

## 0.1.0

### Added

- Initial Aerox 3 Wireless Gen 2 support
- DPI configuration
- DPI stage selection
- Wired and 2.4 GHz polling configuration
