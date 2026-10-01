# Native packaging

Native packages install:

- `steelseriesctl` to `/usr/bin/steelseriesctl`
- `70-steelseries-linux.rules` to `/usr/lib/udev/rules.d/70-steelseries-linux.rules`

Package scriptlets reload udev rules after installation, upgrade, and removal. Reconnecting an already attached mouse or receiver may still be necessary for its desktop-session ACL to refresh.

Build each format in its native distribution, or in a suitable build container using that distribution's packaging tools and the declared build dependencies.

Build artifacts are copied to `dist/`.

---

## Arch Linux / CachyOS

From the repository root:

```bash
./scripts/build-arch-package.sh
```

The helper:

- copies `arch/PKGBUILD` and its install hook into a temporary build directory;
- creates the expected source archive;
- runs a clean `makepkg -sf` build;
- copies the resulting package to `dist/`;
- removes temporary makepkg output after the build.

Inspect the package contents:

```bash
pacman -Qlp dist/steelseries-linux-0.3.0-1-x86_64.pkg.tar.zst
```

Install:

```bash
sudo pacman -U ./dist/steelseries-linux-0.3.0-1-x86_64.pkg.tar.zst
```

---

## Debian / Ubuntu

From the repository root on a Debian-family build system:

```bash
./scripts/build-debian-package.sh
```

The helper runs:

```bash
dpkg-buildpackage -b -us -uc
```

and copies the binary package to `dist/`.

When building in an official Rust container whose toolchain is not registered in dpkg's package database, set:

```bash
STEELSERIES_EXTERNAL_RUST_TOOLCHAIN=1 ./scripts/build-debian-package.sh
```

The declared Debian build dependencies remain unchanged. This flag only skips dpkg's package-database check for the externally supplied Rust toolchain.

Inspect the package contents:

```bash
dpkg-deb -c dist/steelseries-linux_0.3.0-1_amd64.deb
```

Install:

```bash
sudo apt install ./dist/steelseries-linux_0.3.0-1_amd64.deb
```

---

## Fedora

From the repository root on a Fedora build system:

```bash
./scripts/build-fedora-package.sh
```

The helper:

- creates an isolated RPM top directory;
- runs `rpmbuild -ba`;
- copies the resulting binary RPM to `dist/`.

Inspect the package contents:

```bash
rpm -qlp dist/steelseries-linux-0.3.0-1.*.x86_64.rpm
```

Install:

```bash
sudo dnf install ./dist/steelseries-linux-0.3.0-1.*.x86_64.rpm
```

---

## Release checklist

Before publishing release packages:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git diff --check
```

Make sure the workspace version in `Cargo.toml` matches the release version before building packages.

For v0.3.0, package filenames should use `0.3.0` consistently across Arch, Debian, and Fedora artifacts.
