Name:           steelseries-linux
Version:        0.2.0
Release:        1%{?dist}
Summary:        Linux configuration CLI for supported SteelSeries devices

License:        MIT
URL:            https://github.com/3efeoyunda1/steelseries-linux
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  systemd-devel
BuildRequires:  systemd-rpm-macros
Requires:       systemd-udev

%description
SteelSeries Linux provides native userspace configuration for supported
SteelSeries hardware. It currently supports the SteelSeries Aerox 3 Wireless
Gen 2 over wired USB and its linked 2.4 GHz receiver.

%prep
%autosetup

%build
cargo build --release --locked

%check
cargo test --workspace --locked

%install
install -Dm0755 target/release/steelseriesctl \
    %{buildroot}%{_bindir}/steelseriesctl
install -Dm0644 udev/70-steelseries-linux.rules \
    %{buildroot}%{_udevrulesdir}/70-steelseries-linux.rules

%post
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || :
    udevadm trigger --subsystem-match=hidraw --action=change >/dev/null 2>&1 || :
fi

%postun
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules >/dev/null 2>&1 || :
    udevadm trigger --subsystem-match=hidraw --action=change >/dev/null 2>&1 || :
fi

%files
%license LICENSE
%doc README.md
%{_bindir}/steelseriesctl
%{_udevrulesdir}/70-steelseries-linux.rules

%changelog
* Fri Sep 25 2026 3efeoyunda1 <3efeoyunda1@users.noreply.github.com> - 0.2.0-1
- Initial RPM package for SteelSeries Linux.
