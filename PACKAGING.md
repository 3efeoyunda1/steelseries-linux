# Native Packaging

SteelSeries Linux provides native packages for Arch Linux, Debian, Ubuntu, and Fedora.

The application is distributed as two separate packages:

| Package | Description |
|---------|-------------|
| `steelseries-linux` | Command-line utility (`steelseriesctl`) and udev rules |
| `steelseries-linux-gui` | Graphical application, desktop entry |

The GUI package depends on the matching CLI package version. The CLI can be installed independently without GUI dependencies.

## Building Packages

Clone the repository:

```bash
git clone https://github.com/3efeoyunda1/steelseries-linux.git
cd steelseries-linux
```

Install the build dependencies for your distribution, then run the corresponding packaging script.

### Arch Linux / CachyOS

Install the required packages:

```bash
sudo pacman -S --needed \
  base-devel git rust cargo pkgconf \
  fontconfig freetype2 ttf-dejavu \
  systemd libusb hidapi
```

Build both packages:

```bash
./scripts/build-arch-package.sh
```

Generated packages are placed in `dist/arch/`.

### Debian / Ubuntu

Install the required packages:

```bash
sudo apt update
sudo apt install \
  build-essential git dpkg-dev debhelper \
  cargo rustc pkg-config \
  libudev-dev libusb-1.0-0-dev libhidapi-dev \
  libfontconfig1-dev libfreetype6-dev fonts-dejavu-core \
  libwayland-dev libx11-dev libxkbcommon-dev
```

Build both packages:

```bash
./scripts/build-debian-package.sh
```

Generated packages are placed in `dist/debian/`.

**Note:** Some Debian releases provide older Rust toolchains. Make sure your Rust compiler supports the project's dependencies before building.

When using an external Rust toolchain that is not registered with dpkg, the build script supports:

```bash
STEELSERIES_EXTERNAL_RUST_TOOLCHAIN=1 \
  ./scripts/build-debian-package.sh
```

### Fedora

Install the required packages:

```bash
sudo dnf install \
  rpm-build cargo rust gcc gcc-c++ make git \
  pkgconf-pkg-config \
  systemd-devel systemd-rpm-macros \
  libusb1-devel hidapi-devel \
  fontconfig-devel freetype-devel dejavu-sans-fonts \
  wayland-devel libX11-devel libxkbcommon-devel
```

Build both packages:

```bash
./scripts/build-fedora-package.sh
```

Generated packages are placed in `dist/fedora/`.

## Package Verification

Inspect the generated packages using the distribution's native tools.

### Arch Linux

```bash
for pkg in dist/arch/*.pkg.tar.zst; do
  pacman -Qlp "$pkg"
done
```

### Debian / Ubuntu

```bash
for pkg in dist/debian/*.deb; do
  dpkg-deb -c "$pkg"
done
```

### Fedora

```bash
for pkg in dist/fedora/*.rpm; do
  rpm -qlp "$pkg"
done
```

Before distributing packages, verify that both packages were generated, their versions match, and their contents and dependencies are correct.

## Installation

Prebuilt packages and installation instructions are available in the [README](README.md).

Download releases from [GitHub Releases](https://github.com/3efeoyunda1/steelseries-linux/releases).