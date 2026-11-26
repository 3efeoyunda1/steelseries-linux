# Native Packaging

SteelSeries Linux provides native packages for Arch Linux, Debian, Ubuntu, and Fedora.

The application is distributed as two separate packages:

| Package | Description |
|---------|-------------|
| `steelseries-linux` | Command-line utility (`steelseriesctl`) and udev rules |
| `steelseries-linux-gui` | Graphical application, desktop entry |

The GUI package depends on the matching CLI package version. The CLI can be installed independently without GUI dependencies.

## Versioning

The workspace version in `Cargo.toml` is the source of the application version.
The local packaging scripts read it and prepare temporary Arch, Debian, and
Fedora package metadata with that version. They preserve the repository's
packaging files and existing changelog history.

For a standalone Arch release or AUR submission, update `arch/PKGBUILD` and
regenerate `.SRCINFO` from the release version. Those files must provide the
version before makepkg downloads the source archive.

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

## Package Licenses

The project's own source code is MIT-licensed. Both binary packages include
the root `LICENSE` and the full workspace-wide `THIRD_PARTY_LICENSES.md`
report. The report may list dependencies used by the other package or only
during the build. Packaging uses the checked-in report. Update it when
dependency licensing changes.

| Package format | Installed location |
|----------------|--------------------|
| Arch / CachyOS | `/usr/share/licenses/<package-name>/` |
| Debian / Ubuntu | `/usr/share/doc/<package-name>/`, alongside `copyright` |
| Fedora | `/usr/share/licenses/<package-name>/` via RPM `%license` |

Debian packaging also embeds the full report in the installed DEP-5
`copyright` file, keeping the notices available in Debian's standard
copyright location.

Arch and RPM metadata describe the bundled components of each binary package.
The CLI includes the MIT Rust HIDAPI bindings and the BSD-3-Clause native
backend. The GUI includes Slint under its Royalty-free 2.0 terms and other
third-party libraries listed in the report.

Redistribution permission for the SteelSeries product images embedded in the
GUI has not been verified. The project's MIT grant excludes those images.
`LicenseRef-SteelSeries-Images-Unverified` records that unresolved status in
upstream package metadata. It grants no permissions. Review the images and
Slint's license and attribution terms before distribution. Building an RPM
does not establish eligibility for the official Fedora repositories.

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
The native build scripts verify both license files against the repository
copies before copying the packages to `dist/`. They also check Debian's
`copyright` file, RPM's `%license` flags, and the exclusion of local license
generation templates. A missing source file or mismatched package entry fails
the build.

## Installation

Prebuilt packages and installation instructions are available in the [README](README.md).

Download releases from [GitHub Releases](https://github.com/3efeoyunda1/steelseries-linux/releases).
