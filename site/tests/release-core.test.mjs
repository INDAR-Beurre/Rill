import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import {
  API_URL,
  CACHE_TTL_MS,
  buildInstallCommands,
  classifyAssetFilename,
  detectArchitecture,
  fetchLatestRelease,
  formatPackageOptionLabel,
  getSystemRequirements,
  isTrustedGitHubUrl,
  normalizeReleasePayload,
  posixQuote,
  readReleaseCache,
} from '../release-core.mjs';

const TEST_TMP_BASE = process.env.TMPDIR || '/home/alex/.cache';

function makeMemoryStorage() {
  const store = new Map();
  return {
    getItem: (k) => (store.has(k) ? store.get(k) : null),
    setItem: (k, v) => { store.set(k, String(v)); },
    removeItem: (k) => { store.delete(k); },
  };
}

const LIVE_V110_FIXTURE = {
  tag_name: 'v1.1.0',
  name: 'Rill 1.1.0',
  draft: false,
  prerelease: false,
  html_url: 'https://github.com/INDAR-Beurre/Rill/releases/tag/v1.1.0',
  published_at: '2026-11-27T21:15:09Z',
  assets: [
    {
      name: 'Rill-1.1.0-x86_64.AppImage',
      size: 132176376,
      digest: 'sha256:4fa30c48454b8ada52d23e9317cb40537f1ad25be1355a64c5fd88328bb655e2',
      browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.1.0/Rill-1.1.0-x86_64.AppImage',
    },
    {
      name: 'Rill-1.1.0-x86_64.AppImage.sha256',
      size: 93,
      browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.1.0/Rill-1.1.0-x86_64.AppImage.sha256',
    },
  ],
};

test('normalizes actual v1.1.0 release from redirected INDAR-Beurre/Rill URLs', () => {
  const release = normalizeReleasePayload(LIVE_V110_FIXTURE);
  assert.ok(release);
  assert.equal(release.tag, 'v1.1.0');
  assert.equal(release.version, '1.1.0');
  assert.equal(release.htmlUrl, 'https://github.com/INDAR-Beurre/Rill/releases/tag/v1.1.0');
  assert.equal(release.packages.length, 1);

  const pkg = release.packages[0];
  assert.equal(pkg.name, 'Rill-1.1.0-x86_64.AppImage');
  assert.equal(pkg.format, '.AppImage');
  assert.equal(pkg.arch, 'x86_64');
  assert.equal(pkg.sha256, '4fa30c48454b8ada52d23e9317cb40537f1ad25be1355a64c5fd88328bb655e2');
  assert.equal(pkg.sidecar?.name, 'Rill-1.1.0-x86_64.AppImage.sha256');
  assert.equal(pkg.sigSidecar, null);

  const reqs = getSystemRequirements(release, pkg);
  assert.match(reqs.headline, /glibc 2\.44/);
  assert.match(reqs.detail, /1\.2 is in progress/);
});

test('supports all five package formats, multi-arch, SHA256SUMS.txt, and .asc/.sig sidecars', () => {
  const release = normalizeReleasePayload({
    tag_name: 'v1.2.0',
    name: 'Rill 1.2.0',
    draft: false,
    prerelease: false,
    html_url: 'https://github.com/INDAR-Beurre/search-linux/releases/tag/v1.2.0',
    published_at: '2026-12-10T12:00:00Z',
    assets: [
      {
        name: 'rill_1.2.0_amd64.deb',
        size: 42000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/rill_1.2.0_amd64.deb',
      },
      {
        name: 'rill-1.2.0-1.x86_64.rpm',
        size: 43000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/rill-1.2.0-1.x86_64.rpm',
      },
      {
        name: 'rill-1.2.0-1-x86_64.pkg.tar.zst',
        size: 41000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/rill-1.2.0-1-x86_64.pkg.tar.zst',
      },
      {
        name: 'rill-1.2.0-arm64.tar.gz',
        size: 40000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/rill-1.2.0-arm64.tar.gz',
      },
      {
        name: 'Rill-1.2.0-x86_64.AppImage',
        size: 130000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/Rill-1.2.0-x86_64.AppImage',
      },
      {
        name: 'Rill-1.2.0-x86_64.AppImage.asc',
        size: 833,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/Rill-1.2.0-x86_64.AppImage.asc',
      },
      {
        name: 'SHA256SUMS.txt',
        size: 450,
        browser_download_url: 'https://github.com/INDAR-Beurre/search-linux/releases/download/v1.2.0/SHA256SUMS.txt',
      },
    ],
  });

  assert.equal(release.packages.length, 5);
  assert.deepEqual(
    release.packages.map((p) => [p.arch, p.format]),
    [
      ['x86_64', '.AppImage'],
      ['x86_64', '.deb'],
      ['x86_64', '.rpm'],
      ['x86_64', '.pkg.tar.zst'],
      ['aarch64', '.tar.gz'],
    ],
  );
  assert.ok(release.packages.every((p) => p.sidecar?.name === 'SHA256SUMS.txt'));
  assert.equal(release.packages[0].sigSidecar?.name, 'Rill-1.2.0-x86_64.AppImage.asc');

  const debCmds = buildInstallCommands(release.packages[1]);
  assert.doesNotMatch(debCmds.lines[0], /--ignore-missing/);
  assert.match(debCmds.lines[0], /awk -v f='rill_1\.2\.0_amd64\.deb'/);
  assert.match(debCmds.lines[1], /&& sudo apt install -- '\.\/rill_1\.2\.0_amd64\.deb'/);
});

test('disambiguates multiple variants sharing the same architecture and format', () => {
  const release = normalizeReleasePayload({
    tag_name: 'v1.2.0',
    name: 'Rill 1.2.0',
    draft: false,
    prerelease: false,
    html_url: 'https://github.com/INDAR-Beurre/Rill/releases/tag/v1.2.0',
    assets: [
      {
        name: 'Rill-1.2.0-glibc2.35-x86_64.AppImage',
        size: 125000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.2.0/Rill-1.2.0-glibc2.35-x86_64.AppImage',
      },
      {
        name: 'Rill-1.2.0-glibc2.44-x86_64.AppImage',
        size: 132000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.2.0/Rill-1.2.0-glibc2.44-x86_64.AppImage',
      },
      {
        name: 'rill_1.2.0_ubuntu24_amd64.deb',
        size: 41000000,
        browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.2.0/rill_1.2.0_ubuntu24_amd64.deb',
      },
      {
        name: 'rill_1.2.0_debian12_amd64.deb',
        size: 40500000,
        browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.2.0/rill_1.2.0_debian12_amd64.deb',
      },
    ],
  });

  assert.equal(release.packages.length, 4);
  assert.equal(new Set(release.packages.map((p) => p.name)).size, 4);
  assert.match(formatPackageOptionLabel(release.packages[0], true), /glibc2\.35/);
  assert.match(formatPackageOptionLabel(release.packages[1], true), /glibc2\.44/);
  assert.match(formatPackageOptionLabel(release.packages[2], true), /debian12/);
  assert.match(formatPackageOptionLabel(release.packages[3], true), /ubuntu24/);
});

test('rejects untrusted URLs, query/fragment smuggling, path traversal, draft, and prerelease payloads', () => {
  assert.equal(isTrustedGitHubUrl('http://github.com/INDAR-Beurre/Rill/releases/download/v1.1.0/x.AppImage'), null);
  assert.equal(isTrustedGitHubUrl('https://evil.example/INDAR-Beurre/Rill/releases/download/v1.1.0/x.AppImage'), null);
  assert.equal(isTrustedGitHubUrl('https://github.com/other-org/Rill/releases/download/v1.1.0/x.AppImage'), null);
  assert.equal(isTrustedGitHubUrl('https://github.com/INDAR-Beurre/Rill/releases/download/v1.1.0/x.AppImage?foo=1#bar'), null);
  assert.equal(
    isTrustedGitHubUrl('https://github.com/INDAR-Beurre/rill/releases/download/v1.1.0/Rill-1.1.0-x86_64.AppImage', 'asset'),
    'https://github.com/INDAR-Beurre/rill/releases/download/v1.1.0/Rill-1.1.0-x86_64.AppImage',
  );
  assert.match(API_URL, /^https:\/\/api\.github\.com\/repos\/INDAR-Beurre\/Rill\/releases\/latest$/);
  assert.equal(isTrustedGitHubUrl('javascript:alert(1)'), null);
  assert.equal(isTrustedGitHubUrl('https://github.com/INDAR-Beurre/Rill/releases/download/../evil'), null);

  assert.equal(normalizeReleasePayload({ ...LIVE_V110_FIXTURE, draft: true }), null);
  assert.equal(normalizeReleasePayload({ ...LIVE_V110_FIXTURE, prerelease: true }), null);

  const filtered = normalizeReleasePayload({
    ...LIVE_V110_FIXTURE,
    assets: [
      {
        name: '../../evil.AppImage',
        size: 10,
        browser_download_url: 'https://github.com/INDAR-Beurre/Rill/releases/download/v1.1.0/evil.AppImage',
      },
      {
        name: 'Rill-spoofed.AppImage',
        size: 10,
        browser_download_url: 'https://attacker.tld/Rill-spoofed.AppImage',
      },
    ],
  });
  assert.equal(filtered.packages.length, 0);
});

test('safely quotes adversarial asset filenames for POSIX shell commands', () => {
  const nasty = "Rill $(whoami) 'quoted'; rm -rf.AppImage";
  assert.equal(posixQuote(nasty), "'Rill $(whoami) '\\''quoted'\\''; rm -rf.AppImage'");

  const cmds = buildInstallCommands({
    name: nasty,
    format: '.AppImage',
    sha256: null,
    sidecar: { name: `${nasty}.sha256` },
  });
  const block = cmds.lines.join('\n');
  assert.ok(!block.includes('| sh') && !block.includes('| bash'));
  assert.match(cmds.lines[0], /^sha256sum -c -- '\.\//);
  assert.match(cmds.lines[1], /^  && chmod \+x -- '\.\//);
  assert.match(cmds.lines[2], /^  && '\.\//);
});

test('inline digest commands stay short while safely verifying spaced and leading-hyphen files', () => {
  const digest = 'a'.repeat(64);
  const cmds = buildInstallCommands({
    name: '-Rill Browser (1.2)+[rc].AppImage',
    format: '.AppImage',
    sha256: digest,
    sidecar: { name: 'SHA256SUMS.txt' },
  });
  const maxLen = Math.max(...cmds.lines.join('\n').split('\n').map((line) => line.length));
  assert.ok(maxLen <= 120, `expected wrapped inline command <= 120 chars, got ${maxLen}: ${cmds.lines.join('\n')}`);

  const dir = mkdtempSync(join(TEST_TMP_BASE, 'rill-cmd-wrap-'));
  try {
    const fixture = '#!/bin/sh\nprintf executed > executed.marker\n';
    const actualDigest = createHash('sha256').update(fixture).digest('hex');
    writeFileSync(join(dir, '-Rill Browser.AppImage'), fixture);
    const wrapped = buildInstallCommands({
      name: '-Rill Browser.AppImage',
      format: '.AppImage',
      sha256: actualDigest,
      sidecar: null,
    });
    const res = spawnSync('/bin/sh', ['-c', wrapped.lines.join('\n')], { cwd: dir, encoding: 'utf8' });
    assert.equal(res.status, 0, res.stdout + res.stderr);
    assert.equal(readFileSync(join(dir, 'executed.marker'), 'utf8'), 'executed');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('SITE-011/012/013: real /bin/sh execution fails closed on bad checksum and handles spaces, markers, and leading hyphens', () => {
  const fixture = '#!/bin/sh\nprintf executed > executed.marker\n';
  const digest = createHash('sha256').update(fixture).digest('hex');

  function runAppImageCase({ name, sha256 = null, sidecar = null, sidecarBody = null, fileMode = 0o644, writeTarget = true }) {
    const dir = mkdtempSync(join(TEST_TMP_BASE, 'rill-cmd-test-'));
    try {
      if (writeTarget) {
        writeFileSync(join(dir, name), fixture, { mode: fileMode });
      }
      if (sidecar && sidecarBody !== null) {
        writeFileSync(join(dir, sidecar.name), sidecarBody);
      }
      const { lines } = buildInstallCommands({ name, format: '.AppImage', sha256, sidecar });
      const res = spawnSync('/bin/sh', ['-c', lines.join('\n')], { cwd: dir, encoding: 'utf8' });
      const markerPath = join(dir, 'executed.marker');
      return {
        status: res.status,
        output: res.stdout + res.stderr,
        executed: existsSync(markerPath),
        marker: existsSync(markerPath) ? readFileSync(markerPath, 'utf8') : null,
      };
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }

  // SITE-011: mismatched inline digest never chmods or executes fixture
  const badDigest = runAppImageCase({ name: 'Rill.AppImage', sha256: '0'.repeat(64) });
  assert.notEqual(badDigest.status, 0);
  assert.equal(badDigest.executed, false);

  // SITE-011: missing .sha256 sidecar never executes fixture
  const missingSidecar = runAppImageCase({
    name: 'Rill.AppImage',
    sidecar: { name: 'Rill.AppImage.sha256' },
    sidecarBody: null,
  });
  assert.notEqual(missingSidecar.status, 0);
  assert.equal(missingSidecar.executed, false);

  // SITE-011 + SITE-012: missing entry in SHA256SUMS (even if prefix name exists) fails closed
  const missingEntry = runAppImageCase({
    name: 'Rill.AppImage',
    sidecar: { name: 'SHA256SUMS' },
    sidecarBody: `${digest}  Rill Browser.AppImage\n`,
  });
  assert.notEqual(missingEntry.status, 0);
  assert.equal(missingEntry.executed, false);

  // SITE-012: spaces in filename with shared SHA256SUMS passes and runs
  const spaceName = runAppImageCase({
    name: 'Rill Browser.AppImage',
    sidecar: { name: 'SHA256SUMS' },
    sidecarBody: `${digest}  Rill Browser.AppImage\n`,
  });
  assert.equal(spaceName.status, 0, spaceName.output);
  assert.equal(spaceName.marker, 'executed');

  // SITE-012: regex metacharacters + binary '*' marker + CRLF in SHA256SUMS.txt passes cleanly
  const punctName = runAppImageCase({
    name: 'Rill (1.2)+[rc].AppImage',
    sidecar: { name: 'SHA256SUMS.txt' },
    sidecarBody: `${digest} *Rill (1.2)+[rc].AppImage\r\n`,
  });
  assert.equal(punctName.status, 0, punctName.output);
  assert.equal(punctName.marker, 'executed');

  // SITE-013: leading-hyphen filename via inline digest, per-file .sha256, and SHA256SUMS
  const dashDigest = runAppImageCase({ name: '-Rill.AppImage', sha256: digest });
  assert.equal(dashDigest.status, 0, dashDigest.output);
  assert.equal(dashDigest.marker, 'executed');

  const dashSidecar = runAppImageCase({
    name: '-Rill.AppImage',
    sidecar: { name: '-Rill.AppImage.sha256' },
    sidecarBody: `${digest}  ./-Rill.AppImage\n`,
  });
  assert.equal(dashSidecar.status, 0, dashSidecar.output);
  assert.equal(dashSidecar.marker, 'executed');

  const dashShared = runAppImageCase({
    name: '-Rill.AppImage',
    sidecar: { name: 'SHA256SUMS' },
    sidecarBody: `${digest}  -Rill.AppImage\n`,
  });
  assert.equal(dashShared.status, 0, dashShared.output);
  assert.equal(dashShared.marker, 'executed');
});

test('SITE-011/013: package installers and tar fail closed on bad checksum and receive ./-- paths on valid checksum', () => {
  const payload = 'mock-package-bytes\n';
  const digest = createHash('sha256').update(payload).digest('hex');

  for (const [format, filename] of [
    ['.deb', '-rill_1.2.0_amd64.deb'],
    ['.rpm', '-rill-1.2.0.x86_64.rpm'],
    ['.pkg.tar.zst', '-rill-1.2.0.pkg.tar.zst'],
    ['.tar.gz', '-rill-1.2.0.tar.gz'],
  ]) {
    const dir = mkdtempSync(join(TEST_TMP_BASE, 'rill-pkg-test-'));
    const binDir = join(dir, 'bin');
    try {
      spawnSync('mkdir', ['-p', binDir]);
      writeFileSync(join(dir, filename), payload);
      writeFileSync(join(binDir, 'sudo'), '#!/bin/sh\nprintf "%s\\n" "$*" > invoked.marker\n', { mode: 0o755 });
      writeFileSync(join(binDir, 'tar'), '#!/bin/sh\nprintf "%s\\n" "$*" > invoked.marker\n', { mode: 0o755 });
      const env = { ...process.env, PATH: `${binDir}:${process.env.PATH}` };

      const badCmds = buildInstallCommands({ name: filename, format, sha256: 'f'.repeat(64), sidecar: null });
      const badRes = spawnSync('/bin/sh', ['-c', badCmds.lines.join('\n')], { cwd: dir, env, encoding: 'utf8' });
      assert.notEqual(badRes.status, 0);
      assert.equal(existsSync(join(dir, 'invoked.marker')), false, `${format} ran on bad checksum`);

      const okCmds = buildInstallCommands({ name: filename, format, sha256: digest, sidecar: null });
      const okRes = spawnSync('/bin/sh', ['-c', okCmds.lines.join('\n')], { cwd: dir, env, encoding: 'utf8' });
      assert.equal(okRes.status, 0, okRes.stdout + okRes.stderr);
      const invoked = readFileSync(join(dir, 'invoked.marker'), 'utf8');
      assert.match(invoked, new RegExp(`\\./${filename.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }
});

test('SITE-014/M3: static index.html contains no hardcoded 1.1.0 release claims', () => {
  const html = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
  assert.doesNotMatch(html, /1\.1\.0/);
  assert.match(html, /View latest stable release on GitHub/);
});

test('L3: buildInstallCommands warns clearly when no checksum sidecar or digest is published', () => {
  const unverified = buildInstallCommands({
    name: 'Rill-custom.AppImage',
    format: '.AppImage',
    sha256: null,
    sidecar: null,
  });
  assert.equal(unverified.hasVerification, false);
  assert.match(unverified.note, /No checksum was published/);
});

test('fresh 15m cache suppresses repeated API calls unless bypassCache is set', async () => {
  const storage = makeMemoryStorage();
  const clock = 1_700_000_000_000;
  let fetchCount = 0;

  const mockFetch = async (url) => {
    assert.equal(url, API_URL);
    fetchCount += 1;
    return {
      ok: true,
      status: 200,
      json: async () => LIVE_V110_FIXTURE,
    };
  };

  const first = await fetchLatestRelease({ storage, now: () => clock, fetchImpl: mockFetch });
  assert.equal(first.status, 'ready');
  assert.equal(first.fromCache, false);
  assert.equal(fetchCount, 1);

  const second = await fetchLatestRelease({ storage, now: () => clock + 30_000, fetchImpl: mockFetch });
  assert.equal(second.status, 'ready');
  assert.equal(second.fromCache, true);
  assert.equal(second.stale, false);
  assert.equal(fetchCount, 1);

  const explicitRefresh = await fetchLatestRelease({
    storage,
    now: () => clock + 45_000,
    fetchImpl: mockFetch,
    bypassCache: true,
  });
  assert.equal(explicitRefresh.fromCache, false);
  assert.equal(fetchCount, 2);

  assert.equal(readReleaseCache(storage, clock + CACHE_TTL_MS + 1000).fresh, true);
  assert.equal(readReleaseCache(storage, clock + 45_000 + CACHE_TTL_MS + 1).fresh, false);
});

test('handles empty assets, rate limit, body-read timeout, offline, and invalid fallback', async () => {
  const storage = makeMemoryStorage();

  const emptyRes = await fetchLatestRelease({
    storage,
    fetchImpl: async () => ({
      ok: true,
      status: 200,
      json: async () => ({ ...LIVE_V110_FIXTURE, assets: [] }),
    }),
  });
  assert.equal(emptyRes.status, 'empty');
  assert.equal(emptyRes.release.packages.length, 0);

  const clock = 1_700_000_000_000;
  await fetchLatestRelease({
    storage,
    now: () => clock,
    fetchImpl: async () => ({
      ok: true,
      status: 200,
      json: async () => LIVE_V110_FIXTURE,
    }),
  });

  const rateRes = await fetchLatestRelease({
    storage,
    now: () => clock + CACHE_TTL_MS + 5000,
    fetchImpl: async () => ({
      ok: false,
      status: 403,
      headers: new Headers({ 'X-RateLimit-Reset': '1700003600' }),
    }),
  });
  assert.equal(rateRes.status, 'ready');
  assert.equal(rateRes.stale, true);
  assert.equal(rateRes.errorKind, 'ratelimit');
  assert.equal(rateRes.rateLimitResetAt, 1700003600000);

  const coldRateRes = await fetchLatestRelease({
    storage: makeMemoryStorage(),
    fetchImpl: async () => ({
      ok: false,
      status: 429,
      headers: new Headers({ 'X-RateLimit-Reset': '1700003600' }),
    }),
  });
  assert.equal(coldRateRes.status, 'error');
  assert.equal(coldRateRes.errorKind, 'ratelimit');

  const bodyTimeoutRes = await fetchLatestRelease({
    storage: makeMemoryStorage(),
    timeoutMs: 25,
    fetchImpl: async (_url, { signal }) => ({
      ok: true,
      status: 200,
      json: () => new Promise((_resolve, reject) => {
        signal.addEventListener('abort', () => {
          const e = new Error('The operation was aborted');
          e.name = 'AbortError';
          reject(e);
        });
      }),
    }),
  });
  assert.equal(bodyTimeoutRes.status, 'error');
  assert.equal(bodyTimeoutRes.errorKind, 'timeout');

  const offlineRes = await fetchLatestRelease({
    storage: makeMemoryStorage(),
    fetchImpl: async () => {
      throw new TypeError('Failed to fetch');
    },
  });
  assert.equal(offlineRes.status, 'error');
  assert.equal(offlineRes.errorKind, 'network');

  const invalidRes = await fetchLatestRelease({
    storage,
    now: () => clock + CACHE_TTL_MS + 5000,
    fetchImpl: async () => ({
      ok: true,
      status: 200,
      json: async () => ({ tag_name: 'v9.9.9', draft: true }),
    }),
  });
  assert.equal(invalidRes.status, 'ready');
  assert.equal(invalidRes.stale, true);
  assert.equal(invalidRes.errorKind, 'invalid');
});

test('detects package formats and architectures accurately', () => {
  assert.deepEqual(classifyAssetFilename('rill-1.2.0-x86_64.pkg.tar.zst'), {
    kind: 'package',
    format: '.pkg.tar.zst',
  });
  assert.deepEqual(classifyAssetFilename('rill-1.2.0-arm64.tar.gz'), {
    kind: 'package',
    format: '.tar.gz',
  });
  assert.deepEqual(classifyAssetFilename('SHA256SUMS.txt'), {
    kind: 'sidecar',
    format: 'sha256',
  });
  assert.equal(detectArchitecture('rill-1.2.0-amd64.deb'), 'x86_64');
  assert.equal(detectArchitecture('rill-1.2.0-arm64.AppImage'), 'aarch64');
});
