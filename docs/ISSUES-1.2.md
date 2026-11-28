# Rill 1.2 integration issues

This log separates reproduced defects from compatibility requirements and risks
that still need runtime evidence. Nothing here is a declaration of distribution
support. Published 1.1.0 assets remain unchanged.

## VM-001: released AppImage fails before opening on Debian 12

- Status: fixed in the frozen Phase 1 candidate; Debian and Artix GUI regressions passed.
- Severity: launch blocker.
- Artifact: `Rill-1.1.0-x86_64.AppImage`, SHA-256
  `4fa30c48454b8ada52d23e9317cb40537f1ad25be1355a64c5fd88328bb655e2`.
- Environment: actual Debian 12 KVM guest, glibc 2.36, kernel
  `6.1.0-56-cloud-amd64`. Reproduced under Xvfb and Openbox as well as the
  initial command-line launch attempt.
- Observed: exit 127, `libc.so.6: undefined symbol: __pointer_chk_guard,
  version GLIBC_PRIVATE`.
- Cause: the dependency collector copied libc and the dynamic loader from the
  rolling build host despite claiming to leave libc on the host. AppRun's
  library path then pairs a new libc with the guest's older executable loader.
  Excluding libc alone is insufficient: other bundled libraries were compiled
  against glibc 2.44 and need a suitable older build environment.
- Fix owner: Neon packaging engineer.
- Closure evidence: inspect the rebuilt AppImage's library contents and ELF
  requirements, then run the actual GUI smoke suite on Debian 12 and another
  independent runtime with the new artifact hash.
- Additional real KVM reproductions: Fedora 43 (glibc 2.42) and AlmaLinux 9
  encounter the same loader error with the released artifact. Arch reaches the
  application's display initialization on its minimal cloud image; its missing
  display prevents treating that result as a graphical compatibility pass.
- Verified candidate: `Rill-1.1.0-x86_64-glibc2.36-r2.AppImage`, SHA-256
  `2796564ec03fb9f4f508e9db62f1334118769484a7012397f3ea282274aeb857`.
  The packaging engineer's actual Debian 12 KVM smoke passed 11 checks; the
  non-systemd Artix host smoke passed 13, with no GTK warnings. Both rendered a
  local page, executed JavaScript, switched layouts, and accepted real Ctrl+L
  input. These results cover this compatibility candidate, not the upcoming
  integrated 1.2 feature build or the entire distro matrix.

## PKG-002: custom tarball install prefix breaks the desktop launcher

- Status: fixed in the unpublished installer; independent regression passed.
- Severity: desktop launcher unusable after an apparently successful install.
- Scope: `packaging/tarball/install.sh` at initial integration inspection.
- Reproduction: install a temporary package with `--prefix` set to a path whose
  final directory is `Rill Browser & Friends`.
- Observed: installer returns zero but writes an Exec entry containing
  `Rill Browser Exec=search  Friends/bin/search`. The ampersand expands the
  sed replacement's whole match. Spaces would also split an unquoted executable.
- Expected: a correct desktop Exec entry, or an explicit rejection before
  installation of any unsupported prefix.
- Fix owner: Neon packaging engineer.
- Closure evidence: isolated install/uninstall tests using spaces, ampersands,
  and other desktop Exec special characters; verify actual argument parsing,
  preserve existing user profile files, and leave no broken launcher behind.
- Regression result: the new installer successfully launches a fixture through
  GLib/Gio from a prefix containing spaces, `&`, and `$`; its parsed executable
  matches the installed path. Uninstall preserves profile files. Unsupported
  percent signs, quotes, backslashes, tabs, newlines, and empty prefixes are
  rejected before installation. This verifies the installer, not browser GUI
  compatibility.

## COMPAT-003: EL9 needs its own supported build floor

- Status: target requirement identified; native EL9 build pending.
- Targets: AlmaLinux 9, Rocky Linux 9, Oracle Linux 9, and CentOS Stream 9.
- Constraint: glibc 2.34. Debian 12's glibc 2.36 build is not sufficient evidence
  for these targets. A Fedora build with a newer runtime is not sufficient either.
- Required work: build against an actual supported EL9 runtime with WebKitGTK
  4.1, inspect the RPM's generated library dependencies, and run the installed
  package in an EL9 guest. Record unavailable packages as a specific blocker.
- Actual follow-up: a fresh AlmaLinux 9.9 KVM guest verified the r2 artifact
  checksum, installed GTK3/Xvfb/fonts and the missing Wayland server runtime,
  then reproduced `GLIBC_2.35` and `GLIBCXX_3.4.30` symbol failures before the
  GUI opens. Default and CRB repository queries expose WebKitGTK 4.0 with
  libsoup 2.4; they return no providers for Rill's WebKitGTK 4.1/libsoup 3
  development requirements. An additional EPEL query also returned no
  providers for these required APIs. No native
  EL9 RPM has been built, and this candidate does not support EL9.

## COMPAT-004: Debian's WebKit has a direct systemd library dependency

- Status: addressed in Phase 1; non-systemd Artix GUI regression passed.
- Evidence: `readelf -d` in the Debian 12 build guest shows
  `libwebkit2gtk-4.1.so.0` directly requires `libsystemd.so.0`. Debian's
  `libdbus-1.so.3` also requires it.
- Host comparison: the Artix test host has `libelogind.so.0` and no
  `libsystemd.so.0`. The initial new AppImage exclusion list removes both.
- Required work: select and verify a compatible packaging strategy; test the
  resulting image on a non-systemd runtime. Do not assume every excluded
  library exists on every Linux distribution or rename an incompatible library.
- Resolution: the Debian-built image bundles its actual `libsystemd.so.0`
  and `libselinux.so.1` dependencies. The verified r2 candidate above runs on
  Artix without systemd installed.

## PKG-003: compiled WebKit helper paths fail on a different distro layout

- Status: reproduced by the packaging engineer; fixed in the r2 candidate.
- Observed: Debian's WebKit library requests its compiled multiarch helper
  location on Artix. Modern GLib uses `posix_spawn`, which the existing AppRun
  exec hooks do not intercept, so the network/web helper fails with ENOENT.
- Resolution: an AppImage-only preload library redirects the known WebKit
  helper prefixes to the bundled helper directory. Native packages do not
  require this redirection. Debian and Artix GUI checks passed on r2.
- Independent regression: a warning-free shim build passed six actual
  `posix_spawn` checks across Debian, Arch, lib64, and libexec prefixes;
  unrelated paths remain unchanged, including the expected nonexistent-path
  failure. The shim accepts a bundled helper directory containing spaces, but
  the AppRun wrapper still has the separate extraction-path defect below.

## PKG-004: extracted AppDir crashes when its directory contains spaces

- Status: independently reproduced on Artix with the frozen r2 image.
- Reproduction: extract r2, name its directory `Rill Browser`, and launch its
  actual `AppRun` under Xvfb/Openbox using the same smoke harness as the passing
  AppImage test. Extracted AppRun SHA-256:
  `b3ef5773c7b3a303bf2a75057af75eca0095e6039c693f7788e93c969c276587`.
- Observed: the browser starts its bench socket, then exits with signal 5 when
  WebKit cannot spawn the Debian multiarch `WebKitNetworkProcess` path.
- Cause: AppRun skips the entire preload block when its directory contains
  whitespace because the dynamic loader splits preload paths on whitespace.
  That also disables the helper redirection required on other distro layouts.
- Expected: extracted AppDirs work in normal user folders containing spaces;
  preserve helper redirection through a safe loader-compatible mechanism.
- Scope: ordinary extract-and-run already passed because its generated
  extraction directory has no spaces. That pass does not cover manual AppDir
  placement. Keep the r2 artifact frozen; fix in the next candidate and repeat
  both normal and whitespace-path GUI checks.
- The real shim itself handles a space-free symlink path: a host regression
  exported `LD_PRELOAD` pointing at `/home/alex/.cache/rill-preload-link.so`
  and ran two actual `posix_spawn()` calls through Debian and Arch-style
  WebKit prefixes; both returned zero and used the redirected target. This
  distinguishes AppRun's whitespace `LD_PRELOAD` limitation from shim logic.
  Launching the whitespace AppDir through a space-free symlink is a viable
  short-term workaround, not a replacement for fixing AppRun.

## PKG-005: AppImage requires libasound on a minimal Ubuntu desktop fixture

- Status: reproduced by VM tester A on Ubuntu 24.04.5 with the frozen r2 image.
- Observed: initial launch fails because `libasound.so.2` is missing. Installing
  the native package's dependencies allows the same AppImage to pass all 13 GUI
  checks. This is a conditional retest, not a clean-machine AppImage pass.
- Required work: inspect the exclusion/collection strategy for audio libraries,
  repair the portable image, and repeat launch on the unprovisioned fixture.
- Evidence: `baseline/v120-vm/a/URGENT-UBUNTU-APPIMAGE-BUG.md` and
  `baseline/v120-vm/a/ubuntu2404/appimage-failure.txt` in the workspace.

The same clean-host dependency gap was independently observed on Arch:
GUI preparation without `alsa-lib` fails on `libasound.so.2`, while the exact
unchanged r2 image passes 13/13 after installation. Both results are
dependency-conditioned passes, not pristine-host AppImage passes.

## DESKTOP-001: native launcher resolves to a different `search` command

- Status: reproduced by VM tester A on Linux Mint 22.3 Xfce.
- Observed: `gio launch /usr/share/applications/Search.desktop` invokes an
  existing command named `search` that prints a text-search CLI usage message.
  No Rill window appears, so removal/profile preservation cannot be reached.
  The desktop entry itself passes validation.
- Cause under investigation: `Exec=search --url %u` depends on desktop
  `$PATH`, where another executable named `search` can win. The native package
  ships `/usr/bin/search` and a `/usr/bin/rill` symlink.
- Required work: make the launcher invoke the installed browser unambiguously
  without changing the stable profile identity, then retest desktop launch,
  package removal, and profile preservation. Do not count the Mint desktop
  integration as passed before that regression.
- Evidence: `baseline/v120-vm/a/URGENT-MINT-DESKTOP-COLLISION.md` and
  `baseline/v120-vm/a/mint/rill-fixed-r2-validation/native-integration/`.

## Verification scope

Initial independent package inspection passed 31 structure/metadata/ABI checks,
including the newly built Arch archive. These checks do not close PKG-002 and
do not prove VM GUI compatibility. Actual guest reports, artifact hashes, and
installer behavior are separate acceptance evidence.

A second independent inspection passed 55/55 checks, including the installer
regression above, Debian/tarball payload integrity, and extracted AppImage ELF
requirements. The Debian binary requires glibc 2.35; the AppImage's inspected
libraries require at most 2.36 and exclude bundled libc and the dynamic loader.
These are build-floor checks; actual guest rendering and interaction remain
separate release gates.

Memory caveat: the first harness summed every process named WebKit on the
machine. Its Artix aggregate therefore includes unrelated applications and must
not be published as Rill memory usage. The corrected harness selects the tested
browser's process tree and flags inaccessible samples as incomplete; a live
regression confirmed descendant inclusion and exclusion of an unrelated process.
The corrected Artix r2 smoke passed 13/13 checks with no GTK warnings and measured
228.3 MiB PSS across four processes, including 25.5 MiB for the AppImage
extract-and-run runtime. This is one tiny local-page measurement, not an expected
memory budget for arbitrary websites.

Evidence is being collected under the workspace's `baseline/v120-vm/`,
`baseline/v120-packaging/`, and `baseline/v120-integration/` directories. Final
release documentation must include portable summaries rather than rely on
machine-local evidence paths.
