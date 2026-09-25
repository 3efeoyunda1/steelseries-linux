# Native packaging

Native packages install:

- `steelseriesctl` to `/usr/bin/steelseriesctl`
- `70-steelseries-linux.rules` to `/usr/lib/udev/rules.d/70-steelseries-linux.rules`

The package scriptlets reload udev rules after installation, upgrade, and removal. Reconnecting an
already attached mouse or receiver may still be necessary for its session ACL to refresh.

Build each format in its native distribution or a suitable build container with that distribution's
packaging tools and the declared build dependencies installed.

## Arch Linux / CachyOS

From the repository root:

```bash
./scripts/build-arch-package.sh
```

The helper copies `arch/PKGBUILD` and its install hook into a temporary build directory, creates the
expected source archive there, runs a clean `makepkg -sf` build, and copies the package to `dist/`.
Temporary makepkg output is removed after the build.

Inspect with:

```bash
pacman -Qlp dist/steelseries-linux-0.2.0-1-x86_64.pkg.tar.zst
```

Install with:

```bash
sudo pacman -U ./dist/steelseries-linux-0.2.0-1-x86_64.pkg.tar.zst
```

## Debian / Ubuntu

From the repository root on a Debian-family build system:

```bash
./scripts/build-debian-package.sh
```

The helper runs `dpkg-buildpackage -b -us -uc` and copies the binary package to `dist/`.
When building in an official Rust container whose toolchain is not registered in dpkg's package
database, set `STEELSERIES_EXTERNAL_RUST_TOOLCHAIN=1`. The declared Debian build dependencies are
unchanged; this flag only skips dpkg's package-database check for that externally supplied Rust
toolchain.

Inspect with:

```bash
dpkg-deb -c dist/steelseries-linux_0.2.0-1_amd64.deb
```

Install with:

```bash
sudo apt install ./dist/steelseries-linux_0.2.0-1_amd64.deb
```

## Fedora

From the repository root on a Fedora build system:

```bash
./scripts/build-fedora-package.sh
```

The helper creates an isolated RPM top directory, runs `rpmbuild -ba`, and copies the binary RPM to
`dist/`.

Inspect with:

```bash
rpm -qlp dist/steelseries-linux-0.2.0-1.*.x86_64.rpm
```

Install with:

```bash
sudo dnf install ./dist/steelseries-linux-0.2.0-1.*.x86_64.rpm
```
