%global debug_package %{nil}
%define app_version %{?rill_version:%{rill_version}}%{!?rill_version:1.2.0}
%define app_release %{?rill_release:%{rill_release}}%{!?rill_release:1}

Name:           rill
Version:        %{app_version}
Release:        %{app_release}%{?dist}
Summary:        A small, quiet native WebKit browser for Linux
License:        MIT
URL:            https://github.com/INDAR-Beurre/Rill
Source0:        %{name}-%{version}.tar.gz

ExclusiveArch:  x86_64

%if %{undefined prebuilt_binary}
BuildRequires:  cargo >= 1.89
BuildRequires:  rust >= 1.89
BuildRequires:  pkgconfig(gtk+-3.0)
BuildRequires:  pkgconfig(webkit2gtk-4.1) >= 2.40
BuildRequires:  pkgconfig(libsoup-3.0)
BuildRequires:  pkgconfig(sqlite3)
%endif
BuildRequires:  desktop-file-utils

# Shared library SONAMEs (libwebkit2gtk-4.1.so.0, libgtk-3.so.0, libsoup-3.0.so.0,
# libsqlite3.so.0, etc.) are resolved automatically by RPM's ELF dependency generator.
Requires:       hicolor-icon-theme
%if 0%{?suse_version}
Requires:       gstreamer-plugins-base
Requires:       gstreamer-plugins-good
Recommends:     gstreamer-plugins-libav
Recommends:     liberation-fonts
%else
Requires:       gstreamer1-plugins-base
Requires:       gstreamer1-plugins-good
Recommends:     gstreamer1-plugin-libav
Recommends:     liberation-serif-fonts
%endif
Provides:       webclient

%description
Rill is a Rust, GTK 3, and WebKitGTK 4.1 web browser with native top tabs,
an optional vertical tab sidebar, Spaces, reading mode, content blocking,
and local-first history. It uses the shared system WebKitGTK engine rather
than bundling Chromium.

Installs /usr/bin/search and a /usr/bin/rill symlink while preserving the
existing search XDG profile directories (~/.config/search, ~/.local/share/search).

%prep
%autosetup -n %{name}-%{version}

%build
%if %{defined prebuilt_binary}
test -x "%{prebuilt_binary}"
%else
CARGO_BUILD_JOBS=%{?_smp_build_ncpus:%{_smp_build_ncpus}}%{!?_smp_build_ncpus:1} \
  cargo build --release --locked --bin search
%endif

%install
%if %{defined prebuilt_binary}
install -Dm755 "%{prebuilt_binary}" %{buildroot}%{_bindir}/search
%else
install -Dm755 target/release/search %{buildroot}%{_bindir}/search
%endif
ln -sfn search %{buildroot}%{_bindir}/rill
install -Dm644 packaging/Search.desktop %{buildroot}%{_datadir}/applications/Search.desktop
install -Dm644 packaging/search.png %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/search.png
install -Dm644 packaging/icon.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/search.svg
install -Dm644 packaging/search.metainfo.xml %{buildroot}%{_datadir}/metainfo/io.github.indar_beurre.Search.metainfo.xml

%check
desktop-file-validate %{buildroot}%{_datadir}/applications/Search.desktop

%post
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q %{_datadir}/applications || :
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || :
fi

%postun
# Never delete user profile directories (~/.config/search, ~/.local/share/search, ~/.cache/search).
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q %{_datadir}/applications || :
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor || :
fi

%files
%license LICENSE
%{_bindir}/search
%{_bindir}/rill
%{_datadir}/applications/Search.desktop
%{_datadir}/icons/hicolor/512x512/apps/search.png
%{_datadir}/icons/hicolor/scalable/apps/search.svg
%{_datadir}/metainfo/io.github.indar_beurre.Search.metainfo.xml

%changelog
* Sat Nov 28 2026 Rill contributors <https://github.com/INDAR-Beurre/Rill> - 1.2.0-1
- Bump Rill release to 1.2.0 and update canonical repository URL to INDAR-Beurre/Rill

* Fri Nov 27 2026 Rill contributors <https://github.com/INDAR-Beurre/Rill> - 1.1.0-1
- Initial native RPM packaging recipe for Rill (WebKitGTK 4.1, GTK 3, libsoup 3)
