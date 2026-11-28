#!/usr/bin/env python3
"""Offline packaging & ABI verification suite for Rill.

Inspects packaging recipes, scripts, metadata, and built artifacts (.deb, .rpm,
.tar.gz, .pkg.tar.zst, .AppImage) without modifying host packages or user
profile directories.

Usage:
  python3 tools/package_test.py
  python3 tools/package_test.py --artifacts-dir /path/to/artifacts
  python3 tools/package_test.py --artifacts-dir /path/to/artifacts --max-glibc 2.36
  python3 tools/package_test.py --json-out /path/to/report.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HISTORIC_110_APPIMAGE = ROOT / "build" / "Rill-1.1.0-x86_64.AppImage"
HISTORIC_110_SHA256 = "4fa30c48454b8ada52d23e9317cb40537f1ad25be1355a64c5fd88328bb655e2"

FORBIDDEN_APPIMAGE_LIBS = (
    "libc.so.",
    "ld-linux",
    "libm.so.",
    "libmvec.so.",
    "libdl.so.",
    "libpthread.so.",
    "librt.so.",
    "libresolv.so.",
    "libutil.so.",
    "libnss_",
    "libgcc_s.so.",
    "libstdc++.so.",
    "libdrm.so.",
    "libGL.so.",
    "libGLX.so.",
    "libGLdispatch.so.",
    "libEGL.so.",
    "libgbm.so.",
    "libvulkan.so.",
    "libelogind.so.",
)

REQUIRED_SONAMES = {
    "libwebkit2gtk-4.1.so.0",
    "libjavascriptcoregtk-4.1.so.0",
    "libgtk-3.so.0",
    "libsoup-3.0.so.0",
    "libsqlite3.so.0",
    "libc.so.6",
}


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_version_tuple(v: str) -> tuple[int, ...]:
    return tuple(int(x) for x in v.split("."))


def read_cargo_version() -> str:
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        raise RuntimeError("Could not parse version from Cargo.toml")
    return m.group(1)


def inspect_elf(path: Path) -> dict:
    info: dict = {
        "path": str(path),
        "is_elf_x86_64": False,
        "dt_needed": [],
        "glibc_all": [],
        "glibc_required": [],
        "max_glibc_all": "0.0",
        "max_glibc_required": "0.0",
    }
    hdr = subprocess.run(["readelf", "-h", str(path)], capture_output=True, text=True)
    if hdr.returncode != 0:
        return info
    if "ELF64" in hdr.stdout and "X86-64" in hdr.stdout:
        info["is_elf_x86_64"] = True

    dyn = subprocess.run(["readelf", "-d", str(path)], capture_output=True, text=True)
    info["dt_needed"] = re.findall(r"\(NEEDED\)\s+Shared library: \[([^\]]+)\]", dyn.stdout)

    ver = subprocess.run(["readelf", "-V", str(path)], capture_output=True, text=True)
    all_v: set[str] = set()
    req_v: set[str] = set()
    for line in ver.stdout.splitlines():
        for m in re.finditer(r"GLIBC_(\d+\.\d+(?:\.\d+)?)", line):
            v = m.group(1)
            all_v.add(v)
            if "Name: GLIBC_" in line and "WEAK" not in line:
                req_v.add(v)
    if all_v:
        info["glibc_all"] = sorted(all_v, key=parse_version_tuple)
        info["max_glibc_all"] = info["glibc_all"][-1]
    if req_v:
        info["glibc_required"] = sorted(req_v, key=parse_version_tuple)
        info["max_glibc_required"] = info["glibc_required"][-1]
    else:
        info["max_glibc_required"] = info["max_glibc_all"]
    return info


class Suite:
    def __init__(self) -> None:
        self.results: list[dict] = []

    def check(self, name: str, ok: bool, detail: str = "") -> None:
        self.results.append({"name": name, "passed": bool(ok), "detail": detail})
        status = "PASS" if ok else "FAIL"
        suffix = f" — {detail}" if detail else ""
        print(f"[{status}] {name}{suffix}")

    def summary(self) -> tuple[int, int]:
        passed = sum(1 for r in self.results if r["passed"])
        return passed, len(self.results)


def verify_sidecar_sha256(suite: Suite, artifact: Path) -> None:
    sidecar = Path(str(artifact) + ".sha256")
    if not sidecar.is_file():
        suite.check(f"sha256:{artifact.name}", False, "missing .sha256 sidecar")
        return
    line = sidecar.read_text(encoding="utf-8").strip().splitlines()[0]
    parts = line.split()
    expected = parts[0]
    fname = parts[-1].lstrip("*")
    actual = sha256_file(artifact)
    suite.check(
        f"sha256:{artifact.name}",
        actual == expected and fname == artifact.name,
        f"sha256={actual[:12]}... filename={fname}",
    )


def parse_desktop_exec_spec(exec_value: str) -> list[str]:
    """Parse a Desktop Entry Exec= raw value per the FreeDesktop Desktop Entry Spec.

    Step 1: Key-file string unescaping (\\s, \\n, \\t, \\r, \\\\).
    Step 2: Exec argument splitting & double-quote unquoting (\\", \\`, \\$, \\\\).
    Step 3: Field code unescaping (%% -> %).
    """
    # Step 1: Desktop Entry string value unescaping.
    unesc = []
    i = 0
    while i < len(exec_value):
        ch = exec_value[i]
        if ch == "\\" and i + 1 < len(exec_value):
            nxt = exec_value[i + 1]
            mapping = {"s": " ", "n": "\n", "t": "\t", "r": "\r", "\\": "\\"}
            if nxt in mapping:
                unesc.append(mapping[nxt])
                i += 2
                continue
        unesc.append(ch)
        i += 1
    s = "".join(unesc)

    # Step 2: Tokenize Exec arguments per Desktop Entry Spec.
    args: list[str] = []
    cur: list[str] = []
    in_quote = False
    i = 0
    while i < len(s):
        ch = s[i]
        if in_quote:
            if ch == "\\" and i + 1 < len(s) and s[i + 1] in ('"', "`", "$", "\\"):
                cur.append(s[i + 1])
                i += 2
            elif ch == '"':
                in_quote = False
                i += 1
            else:
                cur.append(ch)
                i += 1
        else:
            if ch == '"':
                in_quote = True
                i += 1
            elif ch in (" ", "\t"):
                if cur:
                    args.append("".join(cur))
                    cur = []
                i += 1
            else:
                cur.append(ch)
                i += 1
    if in_quote:
        raise ValueError(f"Unclosed quote in Exec value: {exec_value!r}")
    if cur:
        args.append("".join(cur))

    # Step 3: Field code %% -> % on executable token (leave %u intact).
    return [a.replace("%%", "%") for a in args]


def verify_tarball_installer_edge_cases(suite: Suite) -> None:
    """Test packaging/tarball/install.sh + uninstall.sh directly with special-char prefixes."""
    with tempfile.TemporaryDirectory(prefix="rill-installer-edge-") as tmp:
        td = Path(tmp)
        bundle = td / "bundle"
        (bundle / "bin").mkdir(parents=True)
        (bundle / "share/applications").mkdir(parents=True)
        (bundle / "share/icons/hicolor/512x512/apps").mkdir(parents=True)
        (bundle / "share/icons/hicolor/scalable/apps").mkdir(parents=True)
        (bundle / "share/metainfo").mkdir(parents=True)
        (bundle / "share/licenses/rill").mkdir(parents=True)

        # Dummy executable that prints its own argv[0] and args if launched.
        dummy_bin = bundle / "bin/search"
        dummy_bin.write_text("#!/bin/sh\nprintf '%s\\n' \"$0\" \"$@\"\n", encoding="utf-8")
        dummy_bin.chmod(0o755)
        os.symlink("search", bundle / "bin/rill")
        shutil.copy2(ROOT / "packaging/Search.desktop", bundle / "share/applications/Search.desktop")
        shutil.copy2(ROOT / "packaging/search.png", bundle / "share/icons/hicolor/512x512/apps/search.png")
        shutil.copy2(ROOT / "packaging/icon.svg", bundle / "share/icons/hicolor/scalable/apps/search.svg")
        shutil.copy2(ROOT / "packaging/search.metainfo.xml", bundle / "share/metainfo/io.github.indar_beurre.Search.metainfo.xml")
        shutil.copy2(ROOT / "LICENSE", bundle / "share/licenses/rill/LICENSE")
        shutil.copy2(ROOT / "packaging/tarball/install.sh", bundle / "install.sh")
        shutil.copy2(ROOT / "packaging/tarball/uninstall.sh", bundle / "uninstall.sh")
        (bundle / "install.sh").chmod(0o755)
        (bundle / "uninstall.sh").chmod(0o755)

        fake_home = td / "home"
        profile_file = fake_home / ".config/search/prefs.json"
        profile_file.parent.mkdir(parents=True)
        profile_file.write_text('{"profile":"intact"}\n', encoding="utf-8")
        env = os.environ.copy()
        env["HOME"] = str(fake_home)

        # Case 1: Prefix with spaces, &, and $ (regression for sed '&' expansion & Exec quoting).
        tricky_prefix = td / "Rill Browser & $Friends"
        inst = subprocess.run([str(bundle / "install.sh"), "--prefix", str(tricky_prefix)], env=env, capture_output=True, text=True)
        desktop_out = tricky_prefix / "share/applications/Search.desktop"
        exec_line = ""
        parsed_argv: list[str] = []
        valid_desktop = False
        gio_status = "unavailable"
        exec_run_ok = False
        expected_bin = str(tricky_prefix / "bin/rill")
        if inst.returncode == 0 and desktop_out.is_file():
            for line in desktop_out.read_text(encoding="utf-8").splitlines():
                if line.startswith("Exec="):
                    exec_line = line[len("Exec="):]
                    break
            parsed_argv = parse_desktop_exec_spec(exec_line)
            if shutil.which("desktop-file-validate"):
                v = subprocess.run(["desktop-file-validate", str(desktop_out)], capture_output=True, text=True)
                valid_desktop = v.returncode == 0
            else:
                valid_desktop = True
            # Verify with real GLib.KeyFile + GLib.shell_parse_argv + GioUnix.DesktopAppInfo.
            try:
                import gi  # type: ignore
                gi.require_version("GLib", "2.0")
                from gi.repository import GLib  # type: ignore
                kf = GLib.KeyFile()
                kf.load_from_file(str(desktop_out), GLib.KeyFileFlags.NONE)
                kf_exec = kf.get_string("Desktop Entry", "Exec")
                ok_parse, glib_argv = GLib.shell_parse_argv(kf_exec)
                try:
                    gi.require_version("GioUnix", "2.0")
                    from gi.repository import GioUnix  # type: ignore
                    info = GioUnix.DesktopAppInfo.new_from_filename(str(desktop_out))
                except Exception:
                    gi.require_version("Gio", "2.0")
                    from gi.repository import Gio  # type: ignore
                    info = Gio.DesktopAppInfo.new_from_filename(str(desktop_out))
                if ok_parse and info is not None and list(glib_argv) == [expected_bin, "--url", "%u"]:
                    gio_status = "ok"
                    run_res = subprocess.run(
                        [glib_argv[0], "--url", "https://example.com"],
                        capture_output=True,
                        text=True,
                    )
                    exec_run_ok = run_res.returncode == 0 and run_res.stdout.splitlines() == [
                        expected_bin,
                        "--url",
                        "https://example.com",
                    ]
                else:
                    gio_status = f"mismatch:info={info is not None},argv={glib_argv}"
            except ImportError:
                gio_status = "unavailable"
                if parsed_argv:
                    run_res = subprocess.run(
                        [parsed_argv[0], "--url", "https://example.com"],
                        capture_output=True,
                        text=True,
                    )
                    exec_run_ok = run_res.returncode == 0 and run_res.stdout.splitlines() == [
                        expected_bin,
                        "--url",
                        "https://example.com",
                    ]
            except Exception as exc:
                gio_status = f"error:{exc}"

        exec_ok = (
            inst.returncode == 0
            and valid_desktop
            and parsed_argv == [expected_bin, "--url", "%u"]
            and gio_status in ("ok", "unavailable")
            and exec_run_ok
        )
        suite.check(
            "installer:special-prefix-exec-quoting",
            exec_ok,
            f"Exec={exec_line!r}, parsed={parsed_argv}, validate={valid_desktop}, glib_gio={gio_status}, launched={exec_run_ok}",
        )

        # Verify uninstall removes all installed files cleanly and keeps user profile.
        uninst = subprocess.run([str(bundle / "uninstall.sh"), "--prefix", str(tricky_prefix)], env=env, capture_output=True, text=True)
        suite.check(
            "installer:special-prefix-uninstall",
            uninst.returncode == 0
            and not (tricky_prefix / "bin/search").exists()
            and not desktop_out.exists()
            and profile_file.read_text(encoding="utf-8") == '{"profile":"intact"}\n',
            "tricky prefix uninstalled cleanly and profile preserved",
        )

        # Case 2: Unsupported prefixes (percent, double-quote, backslash, tab, newline, empty) must fail explicitly.
        rejected_all = True
        bad_details = []
        for label, bad in [
            ("percent", str(td / "bad 100% pct")),
            ("quote", str(td / 'bad"quote')),
            ("backslash", str(td / r"bad\slash")),
            ("tab", str(td / "bad\ttab")),
            ("newline", str(td / "bad\nline")),
            ("empty", ""),
        ]:
            r = subprocess.run([str(bundle / "install.sh"), "--prefix", bad], env=env, capture_output=True, text=True)
            if r.returncode == 0:
                rejected_all = False
                bad_details.append(f"{label}:unexpectedly-succeeded")
        suite.check(
            "installer:rejects-unsupported-prefixes",
            rejected_all,
            ", ".join(bad_details) if bad_details else "percent/quote/backslash/tab/newline/empty all rejected",
        )


def verify_relative_out_dir(suite: Suite) -> None:
    """Verify packaging scripts resolve relative --out-dir against caller's CWD."""
    if not shutil.which("makepkg") or not (ROOT / "target/release/search").is_file():
        suite.check("packager:relative-out-dir", True, "skipped (no makepkg or release binary on host)")
        return
    with tempfile.TemporaryDirectory(prefix="rill-rel-out-") as tmp:
        td = Path(tmp)
        res = subprocess.run(
            [str(ROOT / "tools/package-arch.sh"), "--skip-cargo-build", "--out-dir", "./rel-out"],
            cwd=td,
            capture_output=True,
            text=True,
        )
        out_dir = td / "rel-out"
        pkgs = list(out_dir.glob("rill-*.pkg.tar.*")) if out_dir.is_dir() else []
        pkgs = [p for p in pkgs if not p.name.endswith((".sha256", ".abi.json", ".sig"))]
        sha_ok = False
        if pkgs:
            chk = subprocess.run(["sha256sum", "-c", f"{pkgs[0].name}.sha256"], cwd=out_dir, capture_output=True, text=True)
            sha_ok = chk.returncode == 0
        suite.check(
            "packager:relative-out-dir",
            res.returncode == 0 and len(pkgs) == 1 and sha_ok,
            f"cwd-relative ./rel-out produced {[p.name for p in pkgs]}, sha256sum -c={sha_ok}",
        )


def verify_apprun_space_paths(suite: Suite) -> None:
    with tempfile.TemporaryDirectory(prefix="rill-apprun-space-") as tmp:
        td = Path(tmp)
        appdir = td / "Rill Browser"
        appdir.mkdir()
        script = appdir / "AppRun"
        shutil.copy2(ROOT / "packaging/AppRun", script)
        (appdir / "usr/lib").mkdir(parents=True)
        (appdir / "usr/share/search").mkdir(parents=True)
        preload_fixture = Path("/usr/lib/x86_64-linux-gnu/libc.so.6")
        if not preload_fixture.exists():
            preload_fixture = next(Path("/lib/x86_64-linux-gnu").glob("libc.so.6"), Path("/lib64/libc.so.6"))
        for name in ("librill_appimage_preload.so", "libapprun_hooks.so"):
            shutil.copy2(preload_fixture, appdir / "usr/lib" / name)
        (appdir / "usr/bin").mkdir()
        stub = appdir / "usr/bin/search"
        stub.write_text("#!/bin/sh\nprintf '%s\\n' \"$LD_PRELOAD\" > \"$RILL_TEST_OUT\"\nfor arg do printf '%s\\n' \"$arg\" >> \"$RILL_TEST_OUT\"; done\n", encoding="utf-8")
        stub.chmod(0o755)

        env = os.environ.copy()
        env["TMPDIR"] = str(td / "tmp")
        (td / "tmp").mkdir()
        env["RILL_TEST_OUT"] = str(td / "result.txt")
        args = ["--url", "https://example.com/search?q=a b"]
        res = subprocess.run([str(script), *args], env=env, capture_output=True, text=True)
        lines = (td / "result.txt").read_text(encoding="utf-8").splitlines() if (td / "result.txt").exists() else []
        preload = lines[0] if lines else ""
        paths = [Path(p) for p in preload.split(":") if p]
        aliases_ok = bool(paths) and len(paths) == 2
        aliases_ok = aliases_ok and all(" " not in str(p) and p.is_symlink() and p.exists() for p in paths)
        suite.check(
            "guard:AppRun:space-path-preload",
            res.returncode == 0 and aliases_ok and lines[1:] == args,
            f"rc={res.returncode}, preload={preload!r}, argv={lines[1:]!r}, stderr={res.stderr.strip()!r}",
        )


def verify_recipes_and_metadata(suite: Suite, version: str) -> None:
    # 1. Historic Rill 1.1.0 artifact preserved untouched.
    if HISTORIC_110_APPIMAGE.is_file():
        actual = sha256_file(HISTORIC_110_APPIMAGE)
        suite.check(
            "historic:Rill-1.1.0-x86_64.AppImage",
            actual == HISTORIC_110_SHA256,
            f"sha256={actual}",
        )
    else:
        suite.check("historic:Rill-1.1.0-x86_64.AppImage", False, "missing historic 1.1.0 AppImage")

    # 2. Desktop entry validation & legacy profile identity.
    desktop = ROOT / "packaging" / "Search.desktop"
    dtext = desktop.read_text(encoding="utf-8")
    if shutil.which("desktop-file-validate"):
        res = subprocess.run(["desktop-file-validate", str(desktop)], capture_output=True, text=True)
        suite.check("metadata:Search.desktop:validate", res.returncode == 0, res.stderr.strip() or "OK")
    else:
        suite.check("metadata:Search.desktop:exists", desktop.is_file(), "desktop-file-validate unavailable")
    suite.check(
        "metadata:Search.desktop:identity",
        "Name=Rill" in dtext and "Icon=search" in dtext and "StartupWMClass=search" in dtext and "Exec=/usr/bin/rill --url %u" in dtext,
        "Name=Rill, Icon=search, StartupWMClass=search, Exec=/usr/bin/rill --url %u",
    )

    # 3. Icons exist and are valid PNG/SVG.
    png = ROOT / "packaging" / "search.png"
    svg = ROOT / "packaging" / "icon.svg"
    png_bytes = png.read_bytes()
    suite.check("metadata:icon:png", png_bytes.startswith(b"\x89PNG\r\n\x1a\n") and len(png_bytes) > 1000, f"{len(png_bytes)} bytes")
    suite.check("metadata:icon:svg", "<svg" in svg.read_text(encoding="utf-8"), f"{svg.stat().st_size} bytes")

    # 4. AppStream metainfo XML well-formed & ID matches.
    meta = ROOT / "packaging" / "search.metainfo.xml"
    tree = ET.parse(meta)
    root_el = tree.getroot()
    comp_id = (root_el.findtext("id") or "").strip()
    launchable = (root_el.findtext("launchable") or "").strip()
    name = (root_el.findtext("name") or "").strip()
    suite.check(
        "metadata:metainfo.xml",
        comp_id == "io.github.indar_beurre.Search" and launchable == "Search.desktop" and name == "Rill",
        f"id={comp_id}, launchable={launchable}, name={name}",
    )

    # 5. Shell syntax checks on all packaging scripts and hooks.
    bash_scripts = [
        ROOT / "tools" / "appimage.sh",
        ROOT / "tools" / "package-deb.sh",
        ROOT / "tools" / "package-rpm.sh",
        ROOT / "tools" / "package-tarball.sh",
        ROOT / "tools" / "package-arch.sh",
    ]
    sh_scripts = [
        ROOT / "packaging" / "AppRun",
        ROOT / "packaging" / "debian" / "postinst",
        ROOT / "packaging" / "debian" / "postrm",
        ROOT / "packaging" / "tarball" / "install.sh",
        ROOT / "packaging" / "tarball" / "uninstall.sh",
    ]
    for s in bash_scripts:
        res = subprocess.run(["bash", "-n", str(s)], capture_output=True, text=True)
        suite.check(f"syntax:bash:{s.relative_to(ROOT)}", res.returncode == 0, res.stderr.strip() or "OK")
    for s in sh_scripts:
        res = subprocess.run(["sh", "-n", str(s)], capture_output=True, text=True)
        suite.check(f"syntax:sh:{s.relative_to(ROOT)}", res.returncode == 0, res.stderr.strip() or "OK")

    # 6. VM-001 regression guards in tools/appimage.sh & packaging/AppRun.
    appimage_sh = (ROOT / "tools" / "appimage.sh").read_text(encoding="utf-8")
    apprun = (ROOT / "packaging" / "AppRun").read_text(encoding="utf-8")
    suite.check(
        "guard:appimage:excludelist",
        "is_excluded_lib" in appimage_sh and "libc.so.*" in appimage_sh and "ld-linux*.so*" in appimage_sh and "VM-001 guard" in appimage_sh,
        "glibc/ld-linux excludelist and post-stage VM-001 guard present",
    )
    suite.check(
        "guard:appimage:libasound-bundled",
        "libasound.so.*" not in appimage_sh
        and "RESOLVED_ALSA_PATH" in appimage_sh
        and "Missing libasound.so.2 on the build host" in appimage_sh,
        "libasound.so.2 is collected into the AppDir instead of excluded",
    )
    suite.check(
        "guard:appimage:historic-1.1.0",
        "Rill-1.1.0-x86_64.AppImage" in appimage_sh and "Search-redesign-*.AppImage" in appimage_sh,
        "protects 1.0.1, 1.0.2, redesign, and 1.1.0",
    )
    suite.check(
        "guard:AppRun:xdg-fallback",
        "/usr/local/share:/usr/share" in apprun,
        "XDG_DATA_DIRS preserves system share paths when unset",
    )
    verify_apprun_space_paths(suite)

    # 7. Uninstall safety: no rm targeting ~/.config/search or ~/.local/share/search.
    for rel in [
        "packaging/debian/postrm",
        "packaging/tarball/uninstall.sh",
        "packaging/rpm/rill.spec",
    ]:
        txt = (ROOT / rel).read_text(encoding="utf-8")
        unsafe = bool(re.search(r"rm\s+[^\n]*(\.config/search|\.local/share/search|\.cache/search|\$HOME)", txt))
        suite.check(f"safety:uninstall-preserves-profile:{rel}", not unsafe, "never deletes user profile")

    # 8. Debian control template supports t64 transition & WebKitGTK 4.1.
    deb_ctrl = (ROOT / "packaging" / "debian" / "control.in").read_text(encoding="utf-8")
    suite.check(
        "recipe:debian:control.in",
        "Package: rill" in deb_ctrl
        and "libwebkit2gtk-4.1-0" in deb_ctrl
        and "libgtk-3-0 | libgtk-3-0t64" in deb_ctrl
        and "libglib2.0-0 | libglib2.0-0t64" in deb_ctrl
        and "gstreamer1.0-plugins-good" in deb_ctrl,
        "t64-compatible Depends and GStreamer audio plugins declared",
    )

    # 9. RPM spec recipe checks.
    spec = (ROOT / "packaging" / "rpm" / "rill.spec").read_text(encoding="utf-8")
    suite.check(
        "recipe:rpm:rill.spec",
        "Name:           rill" in spec
        and "ExclusiveArch:  x86_64" in spec
        and "pkgconfig(webkit2gtk-4.1)" in spec
        and "%{_bindir}/search" in spec
        and "%{_bindir}/rill" in spec
        and "io.github.indar_beurre.Search.metainfo.xml" in spec,
        "RPM spec metadata, files, and cross-RPM GStreamer deps valid",
    )

    # 10. Arch PKGBUILD & .SRCINFO consistency.
    pkgbuild = (ROOT / "packaging" / "arch" / "PKGBUILD").read_text(encoding="utf-8")
    srcinfo = (ROOT / "packaging" / "arch" / ".SRCINFO").read_text(encoding="utf-8")
    suite.check(
        "recipe:arch:PKGBUILD",
        f"pkgver={version}" in pkgbuild
        and "pkgname=rill" in pkgbuild
        and "'webkit2gtk-4.1'" in pkgbuild
        and "'gst-plugins-good'" in pkgbuild
        and "io.github.indar_beurre.Search.metainfo.xml" in pkgbuild,
        f"PKGBUILD matches version {version}",
    )
    suite.check(
        "recipe:arch:.SRCINFO",
        f"pkgver = {version}" in srcinfo and "pkgname = rill" in srcinfo and "depends = webkit2gtk-4.1" in srcinfo,
        f".SRCINFO matches PKGBUILD version {version}",
    )


def inspect_deb(suite: Suite, deb_path: Path, max_glibc: str) -> None:
    verify_sidecar_sha256(suite, deb_path)
    with tempfile.TemporaryDirectory(prefix="rill-inspect-deb-") as tmp:
        td = Path(tmp)
        res = subprocess.run(["ar", "x", str(deb_path)], cwd=td, capture_output=True, text=True)
        suite.check(f"deb:{deb_path.name}:ar-extract", res.returncode == 0, res.stderr.strip() or "valid ar archive")
        if res.returncode != 0:
            return
        debbin = (td / "debian-binary").read_text(encoding="utf-8").strip() if (td / "debian-binary").is_file() else ""
        suite.check(f"deb:{deb_path.name}:debian-binary", debbin == "2.0", f"version={debbin}")

        ctrl_tar = next(td.glob("control.tar.*"), None)
        data_tar = next(td.glob("data.tar.*"), None)
        suite.check(f"deb:{deb_path.name}:members", ctrl_tar is not None and data_tar is not None, f"{ctrl_tar and ctrl_tar.name}, {data_tar and data_tar.name}")
        if not ctrl_tar or not data_tar:
            return

        ctrl_dir = td / "ctrl"
        data_dir = td / "data"
        ctrl_dir.mkdir()
        data_dir.mkdir()
        subprocess.run(["tar", "-xf", str(ctrl_tar), "-C", str(ctrl_dir)], check=True)
        subprocess.run(["tar", "-xf", str(data_tar), "-C", str(data_dir)], check=True)

        ctrl_txt = (ctrl_dir / "control").read_text(encoding="utf-8")
        suite.check(
            f"deb:{deb_path.name}:control",
            "Package: rill" in ctrl_txt and "Architecture: amd64" in ctrl_txt and "libwebkit2gtk-4.1-0" in ctrl_txt and "libgtk-3-0 | libgtk-3-0t64" in ctrl_txt,
            "Package/Arch/Depends verified",
        )
        suite.check(
            f"deb:{deb_path.name}:maintainer-scripts",
            (ctrl_dir / "postinst").is_file() and (ctrl_dir / "postrm").is_file() and (ctrl_dir / "md5sums").is_file(),
            "postinst, postrm, md5sums present",
        )

        # Verify md5sums for every file in data.tar.
        md5_ok = True
        for line in (ctrl_dir / "md5sums").read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            expected_md5, rel = line.split(None, 1)
            fpath = data_dir / rel.strip()
            if not fpath.is_file():
                md5_ok = False
                break
            actual_md5 = hashlib.md5(fpath.read_bytes()).hexdigest()
            if actual_md5 != expected_md5:
                md5_ok = False
                break
        suite.check(f"deb:{deb_path.name}:md5sums", md5_ok, "all packaged files match DEBIAN/md5sums")

        expected_files = [
            "usr/bin/search",
            "usr/share/applications/Search.desktop",
            "usr/share/icons/hicolor/512x512/apps/search.png",
            "usr/share/icons/hicolor/scalable/apps/search.svg",
            "usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml",
            "usr/share/doc/rill/copyright",
        ]
        missing = [f for f in expected_files if not (data_dir / f).is_file()]
        rill_link = data_dir / "usr/bin/rill"
        link_ok = rill_link.is_symlink() and os.readlink(rill_link) == "search"
        suite.check(f"deb:{deb_path.name}:layout", not missing and link_ok, f"missing={missing}, rill->search={link_ok}")

        elf = inspect_elf(data_dir / "usr/bin/search")
        needed_ok = REQUIRED_SONAMES.issubset(set(elf["dt_needed"]))
        glibc_ok = parse_version_tuple(elf["max_glibc_required"]) <= parse_version_tuple(max_glibc)
        suite.check(
            f"deb:{deb_path.name}:elf-abi",
            elf["is_elf_x86_64"] and needed_ok and glibc_ok,
            f"max_glibc_req={elf['max_glibc_required']} (limit {max_glibc}), max_all={elf['max_glibc_all']}",
        )


def inspect_tarball(suite: Suite, tar_path: Path, max_glibc: str) -> None:
    verify_sidecar_sha256(suite, tar_path)
    with tempfile.TemporaryDirectory(prefix="rill-inspect-tar-") as tmp:
        td = Path(tmp)
        with tarfile.open(tar_path, "r:gz") as tf:
            tf.extractall(td, filter="tar")
        roots = [p for p in td.iterdir() if p.is_dir()]
        suite.check(f"tar:{tar_path.name}:topdir", len(roots) == 1, f"topdir={[p.name for p in roots]}")
        if len(roots) != 1:
            return
        pkg = roots[0]
        required = [
            "bin/search",
            "share/applications/Search.desktop",
            "share/icons/hicolor/512x512/apps/search.png",
            "share/icons/hicolor/scalable/apps/search.svg",
            "share/metainfo/io.github.indar_beurre.Search.metainfo.xml",
            "share/licenses/rill/LICENSE",
            "install.sh",
            "uninstall.sh",
            "README.txt",
            "BUILD_INFO.txt",
        ]
        missing = [f for f in required if not (pkg / f).is_file()]
        link_ok = (pkg / "bin/rill").is_symlink() and os.readlink(pkg / "bin/rill") == "search"
        suite.check(f"tar:{tar_path.name}:layout", not missing and link_ok, f"missing={missing}, rill->search={link_ok}")

        elf = inspect_elf(pkg / "bin/search")
        needed_ok = REQUIRED_SONAMES.issubset(set(elf["dt_needed"]))
        glibc_ok = parse_version_tuple(elf["max_glibc_required"]) <= parse_version_tuple(max_glibc)
        suite.check(
            f"tar:{tar_path.name}:elf-abi",
            elf["is_elf_x86_64"] and needed_ok and glibc_ok,
            f"max_glibc_req={elf['max_glibc_required']} (limit {max_glibc}), max_all={elf['max_glibc_all']}",
        )

        # Round-trip install.sh + uninstall.sh in isolated prefix with fake user profile.
        installed_desktop = (pkg / "share/applications/Search.desktop").read_text(encoding="utf-8")
        shipped_exec_ok = "Exec=search --url %u" in installed_desktop or "Exec=/usr/bin/rill --url %u" in installed_desktop
        suite.check(
            f"tar:{tar_path.name}:shipped-exec",
            shipped_exec_ok,
            "frozen tarballs may still ship Exec=search; new tarballs use /usr/bin/rill",
        )
        prefix = td / "prefix"
        fake_home = td / "home"
        profile_file = fake_home / ".config" / "search" / "prefs.json"
        profile_file.parent.mkdir(parents=True)
        profile_file.write_text('{"keep":true}\n', encoding="utf-8")
        env = os.environ.copy()
        env["HOME"] = str(fake_home)

        inst = subprocess.run([str(pkg / "install.sh"), "--prefix", str(prefix)], env=env, capture_output=True, text=True)
        installed_exec = (prefix / "share/applications/Search.desktop").read_text(encoding="utf-8") if (prefix / "share/applications/Search.desktop").exists() else ""
        absolute_launcher = any(
            value in installed_exec
            for value in (
                "Exec=/usr/bin/rill --url %u",
                f"Exec={prefix}/bin/rill --url %u",
                f"Exec={prefix}/bin/search --url %u",
            )
        )
        installed_ok = (
            inst.returncode == 0
            and (prefix / "bin/search").is_file()
            and (prefix / "bin/rill").is_symlink()
            and (prefix / "share/applications/Search.desktop").is_file()
            and absolute_launcher
        )
        suite.check(f"tar:{tar_path.name}:install-roundtrip", installed_ok, inst.stderr.strip() or "prefix install OK")

        uninst = subprocess.run([str(pkg / "uninstall.sh"), "--prefix", str(prefix)], env=env, capture_output=True, text=True)
        removed_ok = (
            uninst.returncode == 0
            and not (prefix / "bin/search").exists()
            and not (prefix / "bin/rill").exists()
            and not (prefix / "share/applications/Search.desktop").exists()
            and profile_file.is_file()
            and profile_file.read_text(encoding="utf-8") == '{"keep":true}\n'
        )
        suite.check(f"tar:{tar_path.name}:uninstall-preserves-profile", removed_ok, uninst.stderr.strip() or "clean uninstall + profile preserved")


def inspect_arch_pkg(suite: Suite, pkg_path: Path) -> None:
    verify_sidecar_sha256(suite, pkg_path)
    with tempfile.TemporaryDirectory(prefix="rill-inspect-arch-") as tmp:
        td = Path(tmp)
        res = subprocess.run(["tar", "-xf", str(pkg_path), "-C", str(td)], capture_output=True, text=True)
        suite.check(f"arch:{pkg_path.name}:extract", res.returncode == 0, res.stderr.strip() or "OK")
        if res.returncode != 0:
            return
        pkginfo = (td / ".PKGINFO").read_text(encoding="utf-8") if (td / ".PKGINFO").is_file() else ""
        suite.check(
            f"arch:{pkg_path.name}:PKGINFO",
            "pkgname = rill" in pkginfo and "depend = webkit2gtk-4.1" in pkginfo and "depend = gst-plugins-good" in pkginfo and "provides = search" in pkginfo,
            ".PKGINFO metadata and deps verified",
        )
        expected = [
            "usr/bin/search",
            "usr/share/applications/Search.desktop",
            "usr/share/icons/hicolor/512x512/apps/search.png",
            "usr/share/icons/hicolor/scalable/apps/search.svg",
            "usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml",
            "usr/share/licenses/rill/LICENSE",
        ]
        missing = [f for f in expected if not (td / f).is_file()]
        link_ok = (td / "usr/bin/rill").is_symlink() and os.readlink(td / "usr/bin/rill") == "search"
        suite.check(f"arch:{pkg_path.name}:layout", not missing and link_ok, f"missing={missing}, rill->search={link_ok}")

        elf = inspect_elf(td / "usr/bin/search")
        needed_ok = REQUIRED_SONAMES.issubset(set(elf["dt_needed"]))
        suite.check(
            f"arch:{pkg_path.name}:elf-abi",
            elf["is_elf_x86_64"] and needed_ok,
            f"max_glibc_req={elf['max_glibc_required']}, max_all={elf['max_glibc_all']} (Arch/Artix rolling)",
        )


def inspect_rpm(suite: Suite, rpm_path: Path, max_glibc: str) -> None:
    verify_sidecar_sha256(suite, rpm_path)
    magic = rpm_path.read_bytes()[:4]
    suite.check(f"rpm:{rpm_path.name}:magic", magic == b"\xed\xab\xee\xdb", f"magic={magic.hex()}")
    if shutil.which("rpm2cpio") and shutil.which("cpio"):
        with tempfile.TemporaryDirectory(prefix="rill-inspect-rpm-") as tmp:
            td = Path(tmp)
            p1 = subprocess.Popen(["rpm2cpio", str(rpm_path)], stdout=subprocess.PIPE)
            p2 = subprocess.run(["cpio", "-idmu", "--quiet"], cwd=td, stdin=p1.stdout, capture_output=True)
            assert p1.stdout is not None
            p1.stdout.close()
            p1.wait()
            suite.check(f"rpm:{rpm_path.name}:extract", p1.returncode == 0 and p2.returncode == 0, "rpm2cpio OK")
            elf = inspect_elf(td / "usr/bin/search")
            glibc_ok = parse_version_tuple(elf["max_glibc_required"]) <= parse_version_tuple(max_glibc)
            suite.check(
                f"rpm:{rpm_path.name}:elf-abi",
                elf["is_elf_x86_64"] and glibc_ok,
                f"max_glibc_req={elf['max_glibc_required']} (limit {max_glibc})",
            )


def inspect_appimage(suite: Suite, appimage_path: Path, max_glibc: str) -> None:
    verify_sidecar_sha256(suite, appimage_path)
    with tempfile.TemporaryDirectory(prefix="rill-inspect-appimage-") as tmp:
        td = Path(tmp)
        res = subprocess.run([str(appimage_path), "--appimage-extract"], cwd=td, capture_output=True, text=True)
        root = td / "squashfs-root"
        suite.check(f"appimage:{appimage_path.name}:extract", res.returncode == 0 and root.is_dir(), res.stderr.strip() or "extracted")
        if not root.is_dir():
            return

        # VM-001 regression check: NO forbidden host-ABI libraries or dri/.
        lib_dir = root / "usr/lib"
        forbidden = []
        if lib_dir.is_dir():
            for entry in lib_dir.iterdir():
                if entry.is_file() and any(entry.name.startswith(pref) for pref in FORBIDDEN_APPIMAGE_LIBS):
                    forbidden.append(entry.name)
        has_dri = (lib_dir / "dri").is_dir()
        suite.check(
            f"appimage:{appimage_path.name}:vm001-no-forbidden-libs",
            not forbidden and not has_dri,
            f"forbidden={forbidden}, has_dri={has_dri}",
        )

        # Check all ELF files for max GLIBC floor.
        max_found = "0.0"
        culprit = ""
        elf_checked = 0
        for p in list((root / "usr/bin").rglob("*")) + list(lib_dir.rglob("*.so*")):
            if not p.is_file() or p.is_symlink():
                continue
            info = inspect_elf(p)
            if not info["is_elf_x86_64"]:
                continue
            elf_checked += 1
            if parse_version_tuple(info["max_glibc_all"]) > parse_version_tuple(max_found):
                max_found = info["max_glibc_all"]
                culprit = str(p.relative_to(root))
        suite.check(
            f"appimage:{appimage_path.name}:glibc-floor",
            elf_checked > 0 and parse_version_tuple(max_found) <= parse_version_tuple(max_glibc),
            f"max_glibc={max_found} ({culprit}), limit={max_glibc}, elves_checked={elf_checked}",
        )

        # Required assets, licenses, icons, metainfo, WebKit helpers, GStreamer plugins.
        required = [
            "AppRun",
            "Search.desktop",
            "search.png",
            ".DirIcon",
            "usr/bin/search",
            "usr/lib/librill_appimage_preload.so",
            "usr/lib/webkit2gtk-4.1/WebKitWebProcess",
            "usr/lib/webkit2gtk-4.1/WebKitNetworkProcess",
            "usr/lib/gstreamer-1.0/libgstautodetect.so",
            "usr/lib/gstreamer-1.0/libgstaudiofx.so",
            "usr/share/licenses/search/LiberationSerif-LICENSE",
            "usr/share/metainfo/io.github.indar_beurre.Search.metainfo.xml",
            "usr/share/icons/hicolor/512x512/apps/search.png",
            "usr/share/icons/hicolor/scalable/apps/search.svg",
        ]
        missing = [f for f in required if not (root / f).exists()]
        suite.check(f"appimage:{appimage_path.name}:layout", not missing, f"missing={missing}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Offline packaging and ABI verification for Rill")
    ap.add_argument("--artifacts-dir", type=Path, default=None, help="Directory of built packages to inspect")
    ap.add_argument("--max-glibc", default="2.36", help="Maximum GLIBC version for portable artifacts (default: 2.36)")
    ap.add_argument("--json-out", type=Path, default=None, help="Write machine-readable JSON report")
    args = ap.parse_args()

    version = read_cargo_version()
    suite = Suite()
    verify_recipes_and_metadata(suite, version)
    verify_tarball_installer_edge_cases(suite)
    verify_relative_out_dir(suite)

    if args.artifacts_dir and args.artifacts_dir.is_dir():
        for p in sorted(args.artifacts_dir.iterdir()):
            if not p.is_file():
                continue
            name = p.name
            if name.endswith(".deb"):
                inspect_deb(suite, p, args.max_glibc)
            elif name.endswith(".tar.gz") and name.startswith("Rill-"):
                inspect_tarball(suite, p, args.max_glibc)
            elif ".pkg.tar." in name and not name.endswith((".sha256", ".abi.json", ".sig")):
                inspect_arch_pkg(suite, p)
            elif name.endswith(".rpm"):
                inspect_rpm(suite, p, args.max_glibc)
            elif name.endswith(".AppImage"):
                inspect_appimage(suite, p, args.max_glibc)

    passed, total = suite.summary()
    print(f"\nSummary: {passed}/{total} checks passed")

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(
            json.dumps({"version": version, "passed": passed, "total": total, "results": suite.results}, indent=2) + "\n",
            encoding="utf-8",
        )

    return 0 if passed == total else 1


if __name__ == "__main__":
    sys.exit(main())
