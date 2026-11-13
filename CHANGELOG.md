# Changelog

## 0.4.1

### Added

- Persistent GUI settings, known-device registry, and configurable device priority
- Configurable background device discovery and battery monitoring shared by the GUI and system tray
- System tray with battery and charging icons, selectable colored/monochrome icon styles, and hide/restore behavior
- Optional user-level XDG autostart with a tray-aware `--autostart` launch mode
- Separate CLI and optional GUI packages for Arch Linux, Debian, and Fedora, with matching-version GUI-to-CLI dependencies
- Single-instance GUI activation: launching again opens the existing window, including when hidden in the tray

### Changed

- GUI executable, desktop entry, application ID, and icon now use `steelseries-linux-gui`; the visible application name remains SteelSeries Linux
- Existing application-managed autostart entries migrate to the renamed GUI executable
- Start-at-login launches now always start hidden in the system tray, enabling the tray when needed
- DPI sliders use capability-selected curves, snapped numeric previews, and clickable reference labels; persisted stages save on release without writes during dragging
- Sleep Timer exposes a recommended GUI range separately from the full CLI capability range

### Fixed

- Fix Fedora 45 GUI RPM installation by declaring the correct Wayland, X11, and XKB runtime library dependencies
- Build Fedora RPM packages in a Fedora 45 container with distribution-tagged package versions
- Avoid a ghost KDE Wayland taskbar entry on hidden autostart by creating the main window only when opened, while preserving tray and single-instance activation
- Reapply native window constraints after tray restore to preserve the fixed-size GUI
- Set the desktop application identity so Wayland desktops can associate the window with its icon
- Keep generic sliders and numeric inputs synchronized after manual edits
- Preserve cached battery percentage and charging state during device discovery

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

### Changed

- HID response filtering
- Physical-device deduplication
- Receiver link validation

## 0.1.0

### Added

- Initial Aerox 3 Wireless Gen 2 support
- DPI configuration
- DPI stage selection
- Wired and 2.4 GHz polling configuration
