Name:           steelseries-linux
Version:        0.4.0
Release:        1%{?dist}
Summary:        Linux configuration CLI for supported SteelSeries devices

License:        MIT
URL:            https://github.com/3efeoyunda1/steelseries-linux
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  fontconfig-devel
BuildRequires:  systemd-devel
BuildRequires:  systemd-rpm-macros
Requires:       systemd-udev

%description
SteelSeries Linux provides native userspace configuration for supported
SteelSeries hardware. It currently supports the SteelSeries Aerox 3 Wireless
Gen 2 over wired USB and its linked 2.4 GHz receiver.

%package gui
Summary:        Graphical application for SteelSeries Linux
Requires:       %{name}%{?_isa} = %{version}-%{release}

%description gui
SteelSeries Linux GUI provides an optional graphical interface for the
steelseriesctl JSON API. Device access and validation remain in the main
steelseries-linux package.

%prep
%autosetup

%build
cargo build --release --locked -p steelseries-cli -p steelseries-gui

%check
cargo test --workspace --locked

%install
install -Dm0755 target/release/steelseriesctl \
    %{buildroot}%{_bindir}/steelseriesctl
install -Dm0644 udev/70-steelseries-linux.rules \
    %{buildroot}%{_udevrulesdir}/70-steelseries-linux.rules
install -Dm0755 target/release/steelseries-gui \
    %{buildroot}%{_bindir}/steelseries-gui
install -Dm0644 assets/app/steelseries-linux.desktop \
    %{buildroot}%{_datadir}/applications/steelseries-linux.desktop
install -Dm0644 assets/app/steelseries-linux.svg \
    %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/steelseries-linux.svg

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

%files gui
%license LICENSE
%{_bindir}/steelseries-gui
%{_datadir}/applications/steelseries-linux.desktop
%{_datadir}/icons/hicolor/scalable/apps/steelseries-linux.svg

%changelog
* Thu Nov 05 2026 3efeoyunda1 <3efeoyunda1@users.noreply.github.com> - 0.4.0-1
- Add the device capability, description, and snapshot API foundation.

* Fri Oct 02 2026 3efeoyunda1 <3efeoyunda1@users.noreply.github.com> - 0.3.0-1
- Release 0.3.0

* Fri Sep 25 2026 3efeoyunda1 <3efeoyunda1@users.noreply.github.com> - 0.2.0-1
- Initial Fedora package for SteelSeries Linux.
