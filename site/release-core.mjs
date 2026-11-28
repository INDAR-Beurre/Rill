export const API_URL = 'https://api.github.com/repos/INDAR-Beurre/Rill/releases/latest';
export const RELEASES_FALLBACK_URL = 'https://github.com/INDAR-Beurre/Rill/releases/latest';
export const SOURCE_BUILD_URL = 'https://github.com/INDAR-Beurre/Rill#build-from-source';
export const CACHE_KEY = 'rill.release.latest.v1';
export const CACHE_TTL_MS = 15 * 60 * 1000;
export const DEFAULT_TIMEOUT_MS = 8000;

const TRUSTED_REPO_PREFIXES = [
  '/INDAR-Beurre/rill/',
  '/INDAR-Beurre/search-linux/',
];

const PACKAGE_FORMAT_ORDER = [
  '.AppImage',
  '.deb',
  '.rpm',
  '.pkg.tar.zst',
  '.tar.gz',
];

const ARCH_ORDER = ['x86_64', 'aarch64', 'linux', 'any'];

/**
 * Validates that a URL is a clean HTTPS GitHub URL belonging to the Rill / search-linux repository.
 * Matches the owner/repo prefix case-insensitively while rejecting credentials, ports, path traversal, queries, and fragments.
 * @param {unknown} rawUrl
 * @param {'release' | 'asset'} kind
 * @returns {string | null}
 */
export function isTrustedGitHubUrl(rawUrl, kind = 'asset') {
  if (typeof rawUrl !== 'string' || rawUrl.length === 0 || rawUrl.length > 1024) {
    return null;
  }
  let parsed;
  try {
    parsed = new URL(rawUrl);
  } catch {
    return null;
  }
  if (parsed.protocol !== 'https:' || parsed.hostname.toLowerCase() !== 'github.com') {
    return null;
  }
  if (parsed.username || parsed.password || parsed.port || parsed.search || parsed.hash) {
    return null;
  }
  const path = parsed.pathname;
  if (path.includes('..') || path.includes('\\')) {
    return null;
  }
  const lowerPath = path.toLowerCase();
  const matchedPrefix = TRUSTED_REPO_PREFIXES.find((prefix) => lowerPath.startsWith(prefix.toLowerCase()));
  if (!matchedPrefix) {
    return null;
  }
  const suffix = path.slice(matchedPrefix.length);
  if (kind === 'release') {
    return suffix.startsWith('releases/tag/') || suffix === 'releases/latest'
      ? parsed.toString()
      : null;
  }
  return suffix.startsWith('releases/download/') ? parsed.toString() : null;
}

/**
 * Validates an asset filename so it cannot escape directories or carry control characters.
 * @param {unknown} name
 * @returns {string | null}
 */
export function sanitizeAssetName(name) {
  if (typeof name !== 'string') {
    return null;
  }
  const trimmed = name.trim();
  if (
    trimmed.length === 0 ||
    trimmed.length > 180 ||
    trimmed.includes('/') ||
    trimmed.includes('\\') ||
    trimmed.includes('..') ||
    /[\u0000-\u001f\u007f]/.test(trimmed)
  ) {
    return null;
  }
  return trimmed;
}

/**
 * Validates a release tag or display name as safe plain text.
 * @param {unknown} value
 * @returns {string}
 */
export function sanitizeLabel(value) {
  if (typeof value !== 'string') {
    return '';
  }
  return value.replace(/[\u0000-\u001f\u007f]/g, '').trim().slice(0, 120);
}

/**
 * Validates GitHub's asset digest string ("sha256:<64 hex chars>").
 * @param {unknown} digest
 * @returns {string | null}
 */
export function sanitizeSha256Digest(digest) {
  if (typeof digest !== 'string') {
    return null;
  }
  const match = digest.trim().toLowerCase().match(/^sha256:([0-9a-f]{64})$/);
  return match ? match[1] : null;
}

/**
 * Identifies supported Linux package formats or sidecar files.
 * Checks multi-extension suffixes before single-extension suffixes.
 * @param {string} filename
 * @returns {{ kind: 'package' | 'sidecar', format: string } | null}
 */
export function classifyAssetFilename(filename) {
  const lower = filename.toLowerCase();
  if (
    lower.endsWith('.sha256') ||
    lower.endsWith('.sha256sum') ||
    lower === 'sha256sums' ||
    lower === 'sha256sums.txt'
  ) {
    return { kind: 'sidecar', format: 'sha256' };
  }
  if (lower.endsWith('.asc') || lower.endsWith('.sig')) {
    return { kind: 'sidecar', format: 'sig' };
  }
  if (lower.endsWith('.pkg.tar.zst')) {
    return { kind: 'package', format: '.pkg.tar.zst' };
  }
  if (lower.endsWith('.tar.gz')) {
    return { kind: 'package', format: '.tar.gz' };
  }
  if (lower.endsWith('.appimage')) {
    return { kind: 'package', format: '.AppImage' };
  }
  if (lower.endsWith('.deb')) {
    return { kind: 'package', format: '.deb' };
  }
  if (lower.endsWith('.rpm')) {
    return { kind: 'package', format: '.rpm' };
  }
  return null;
}

/**
 * Extracts normalized architecture from an asset filename.
 * @param {string} filename
 * @returns {string}
 */
export function detectArchitecture(filename) {
  const lower = filename.toLowerCase();
  if (/(^|[-_.])(x86_64|amd64)([-_.]|$)/.test(lower)) {
    return 'x86_64';
  }
  if (/(^|[-_.])(aarch64|arm64)([-_.]|$)/.test(lower)) {
    return 'aarch64';
  }
  if (/(^|[-_.])(noarch|all|any)([-_.]|$)/.test(lower)) {
    return 'any';
  }
  return 'linux';
}

/**
 * Formats byte count into readable MiB/KiB text.
 * @param {number} bytes
 * @returns {string}
 */
export function formatBytes(bytes) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return 'Size unavailable';
  }
  const mib = bytes / (1024 * 1024);
  if (mib >= 10) {
    return `${mib.toFixed(1)} MiB`;
  }
  if (mib >= 1) {
    return `${mib.toFixed(2)} MiB`;
  }
  const kib = bytes / 1024;
  return `${Math.round(kib)} KiB`;
}

/**
 * POSIX shell single-quote escaping so filenames can never inject shell tokens.
 * @param {string} value
 * @returns {string}
 */
export function posixQuote(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

/**
 * Formats an unambiguous selector label for a package asset.
 * Includes the distinguishing filename when multiple variants share (arch, format).
 * @param {{ name: string, format: string, sizeLabel: string }} pkg
 * @param {boolean} hasSiblingVariant
 * @returns {string}
 */
export function formatPackageOptionLabel(pkg, hasSiblingVariant = false) {
  if (hasSiblingVariant) {
    return `${pkg.format}: ${pkg.name} (${pkg.sizeLabel})`;
  }
  return `${pkg.format} (${pkg.sizeLabel})`;
}

/**
 * Normalizes and validates a raw GitHub release JSON object.
 * Rejects drafts, prereleases, and untrusted URLs.
 * @param {unknown} raw
 * @returns {object | null}
 */
export function normalizeReleasePayload(raw) {
  if (!raw || typeof raw !== 'object') {
    return null;
  }
  if (raw.draft === true || raw.prerelease === true) {
    return null;
  }

  const tag = sanitizeLabel(raw.tag_name);
  const title = sanitizeLabel(raw.name) || tag;
  const htmlUrl = isTrustedGitHubUrl(raw.html_url, 'release') || RELEASES_FALLBACK_URL;
  const publishedAt = typeof raw.published_at === 'string' ? raw.published_at : '';

  if (!tag) {
    return null;
  }

  const rawAssets = Array.isArray(raw.assets) ? raw.assets : [];
  const sidecarsByTarget = new Map();
  const sigsByTarget = new Map();
  let sharedSha256Sums = null;
  const packages = [];

  for (const item of rawAssets) {
    if (!item || typeof item !== 'object') {
      continue;
    }
    const name = sanitizeAssetName(item.name);
    const downloadUrl = isTrustedGitHubUrl(item.browser_download_url, 'asset');
    if (!name || !downloadUrl) {
      continue;
    }
    const classification = classifyAssetFilename(name);
    if (!classification) {
      continue;
    }

    if (classification.kind === 'sidecar') {
      if (classification.format === 'sha256') {
        const lower = name.toLowerCase();
        if (lower === 'sha256sums' || lower === 'sha256sums.txt') {
          sharedSha256Sums = { name, url: downloadUrl };
        } else {
          const targetName = name.replace(/\.(sha256sum|sha256)$/i, '');
          sidecarsByTarget.set(targetName, { name, url: downloadUrl });
        }
      } else if (classification.format === 'sig') {
        const targetName = name.replace(/\.(asc|sig)$/i, '');
        sigsByTarget.set(targetName, { name, url: downloadUrl });
      }
      continue;
    }

    packages.push({
      name,
      url: downloadUrl,
      format: classification.format,
      arch: detectArchitecture(name),
      sizeBytes: typeof item.size === 'number' && item.size > 0 ? item.size : 0,
      sizeLabel: formatBytes(item.size),
      sha256: sanitizeSha256Digest(item.digest),
      sidecar: null,
      sigSidecar: null,
    });
  }

  for (const pkg of packages) {
    pkg.sidecar = sidecarsByTarget.get(pkg.name) || sharedSha256Sums || null;
    pkg.sigSidecar = sigsByTarget.get(pkg.name) || null;
  }

  packages.sort((a, b) => {
    const archDiff =
      (ARCH_ORDER.indexOf(a.arch) === -1 ? 99 : ARCH_ORDER.indexOf(a.arch)) -
      (ARCH_ORDER.indexOf(b.arch) === -1 ? 99 : ARCH_ORDER.indexOf(b.arch));
    if (archDiff !== 0) {
      return archDiff;
    }
    const fmtDiff =
      (PACKAGE_FORMAT_ORDER.indexOf(a.format) === -1 ? 99 : PACKAGE_FORMAT_ORDER.indexOf(a.format)) -
      (PACKAGE_FORMAT_ORDER.indexOf(b.format) === -1 ? 99 : PACKAGE_FORMAT_ORDER.indexOf(b.format));
    if (fmtDiff !== 0) {
      return fmtDiff;
    }
    return a.name.localeCompare(b.name);
  });

  return {
    tag,
    version: tag.replace(/^v/i, ''),
    title,
    htmlUrl,
    publishedAt,
    packages,
  };
}

/**
 * Builds portable, safely-quoted terminal verification and launch/install commands for a selected asset.
 * Prefers per-asset .sha256 sidecar, else API-verified sha256 digest via printf, else exact-match awk extraction from SHA256SUMS.
 * Chains steps with && so any verification or permission failure aborts before execution.
 * @param {{ name: string, format: string, sha256: string | null, sidecar: { name: string } | null }} pkg
 * @returns {{ lines: string[], note: string, hasVerification: boolean }}
 */
export function buildInstallCommands(pkg) {
  const rawName = pkg.name;
  const relPath = `./${rawName}`;
  const quotedRelPath = posixQuote(relPath);
  const steps = [];
  let hasVerification = false;

  const inlineDigestCommands = (digest) => [
    `digest='${digest.slice(0, 32)}'\\
'${digest.slice(32)}' \\
  && file=${quotedRelPath} \\
  && printf '%s  %s\\n' "$digest" "$file" \\\n    | sha256sum -c -`,
  ];

  if (pkg.sidecar && pkg.sidecar.name) {
    const lowerSidecar = pkg.sidecar.name.toLowerCase();
    const quotedRelSidecar = posixQuote(`./${pkg.sidecar.name}`);
    if (lowerSidecar === 'sha256sums' || lowerSidecar === 'sha256sums.txt') {
      hasVerification = true;
      if (pkg.sha256) {
        steps.push(...inlineDigestCommands(pkg.sha256));
      } else {
        steps.push(
          `sum_line="$(awk -v f=${posixQuote(rawName)} -v fp=${quotedRelPath} '/^[0-9a-fA-F]{64} [ *]/ { n = substr($0, 67); sub(/\\r$/, "", n); if (n == f || n == fp) { print; found = 1; exit 0 } } END { if (!found) exit 1 }' ${quotedRelSidecar})" \\\n    && printf '%s\\n' "$sum_line" | sha256sum -c -`,
        );
      }
    } else {
      hasVerification = true;
      steps.push(`sha256sum -c -- ${quotedRelSidecar}`);
    }
  } else if (pkg.sha256) {
    hasVerification = true;
    steps.push(...inlineDigestCommands(pkg.sha256));
  }

  let note = 'Run in the folder containing the downloaded file.';
  switch (pkg.format) {
    case '.AppImage':
      steps.push(`chmod +x -- ${quotedRelPath}`);
      steps.push(quotedRelPath);
      note = hasVerification
        ? 'No root access required. On hosts without FUSE, run with --appimage-extract-and-run.'
        : 'No checksum was published for this asset; verify on GitHub Releases before running. On hosts without FUSE, use --appimage-extract-and-run.';
      break;
    case '.deb':
      steps.push(`sudo apt install -- ${quotedRelPath}`);
      note = hasVerification
        ? 'Installs the Debian package and system runtime dependencies via apt.'
        : 'No checksum was published for this asset; verify on GitHub Releases before installing via apt.';
      break;
    case '.rpm':
      steps.push(`sudo dnf install -- ${quotedRelPath}`);
      note = hasVerification
        ? 'Installs the RPM package and system runtime dependencies via dnf.'
        : 'No checksum was published for this asset; verify on GitHub Releases before installing via dnf.';
      break;
    case '.pkg.tar.zst':
      steps.push(`sudo pacman -U -- ${quotedRelPath}`);
      note = hasVerification
        ? 'Installs the local Arch package via pacman.'
        : 'No checksum was published for this asset; verify on GitHub Releases before installing via pacman.';
      break;
    case '.tar.gz':
      steps.push(`tar -xzf ${quotedRelPath}`);
      note = hasVerification
        ? 'Extracts the archive into the current directory.'
        : 'No checksum was published for this asset; verify on GitHub Releases before extracting.';
      break;
    default:
      break;
  }

  const lines = steps.map((step, index) => {
    const prefix = index === 0 ? '' : '  && ';
    const suffix = index < steps.length - 1 ? ' \\' : '';
    return `${prefix}${step}${suffix}`;
  });

  return { lines, note, hasVerification };
}

/**
 * Returns honest architecture, glibc, and distro notes for the release and selected asset.
 * @param {{ version: string, packages: Array<{ arch: string, format: string }> }} release
 * @param {{ arch: string, format: string } | null} selectedPkg
 * @returns {{ headline: string, detail: string }}
 */
export function getSystemRequirements(release, selectedPkg) {
  const version = release && release.version ? release.version : '';
  const arch = selectedPkg ? selectedPkg.arch : 'x86_64';
  const format = selectedPkg ? selectedPkg.format : '.AppImage';

  if (version.startsWith('1.1.') && format === '.AppImage') {
    return {
      headline: `Requires Linux ${arch}, Wayland or X11, and glibc 2.44 or newer.`,
      detail: 'The 1.1.0 AppImage bundles WebKitGTK 4.1 and requires glibc 2.44+ (native source builds use system WebKitGTK). Broader older-glibc compatibility for 1.2 is in progress.',
    };
  }

  return {
    headline: `Requires Linux ${arch} with a Wayland or X11 desktop session.`,
    detail: `Selected format: ${format}. Check the release notes for distribution runtime requirements.`,
  };
}

/**
 * Reads cached release metadata if present and syntactically valid.
 * @param {Storage | null | undefined} storage
 * @param {number} now
 * @returns {{ release: object, fetchedAt: number, fresh: boolean } | null}
 */
export function readReleaseCache(storage, now = Date.now()) {
  if (!storage || typeof storage.getItem !== 'function') {
    return null;
  }
  try {
    const raw = storage.getItem(CACHE_KEY);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed.fetchedAt !== 'number' || !parsed.release) {
      return null;
    }
    const release = normalizeReleasePayload(parsed.raw || parsed.release);
    if (!release) {
      return null;
    }
    const age = now - parsed.fetchedAt;
    return {
      release,
      fetchedAt: parsed.fetchedAt,
      fresh: age >= 0 && age < CACHE_TTL_MS,
    };
  } catch {
    return null;
  }
}

/**
 * Writes validated raw payload + timestamp to localStorage safely.
 * @param {Storage | null | undefined} storage
 * @param {object} rawPayload
 * @param {object} normalizedRelease
 * @param {number} now
 */
export function writeReleaseCache(storage, rawPayload, normalizedRelease, now = Date.now()) {
  if (!storage || typeof storage.setItem !== 'function') {
    return;
  }
  try {
    storage.setItem(
      CACHE_KEY,
      JSON.stringify({
        fetchedAt: now,
        raw: rawPayload,
        release: normalizedRelease,
      }),
    );
  } catch {
    // Ignore storage quota or private-mode failures.
  }
}

/**
 * Fetches latest published stable release with fresh-cache suppression, bounded timeout through body read, and structured error reporting.
 * @param {object} options
 */
export async function fetchLatestRelease({
  fetchImpl = globalThis.fetch,
  storage = null,
  now = () => Date.now(),
  timeoutMs = DEFAULT_TIMEOUT_MS,
  apiUrl = API_URL,
  bypassCache = false,
} = {}) {
  const timestamp = now();
  const cached = readReleaseCache(storage, timestamp);

  if (!bypassCache && cached && cached.fresh && cached.release.packages.length > 0) {
    return {
      status: 'ready',
      release: cached.release,
      stale: false,
      fromCache: true,
      fetchedAt: cached.fetchedAt,
      errorKind: null,
      errorDetail: null,
    };
  }

  if (typeof fetchImpl !== 'function') {
    if (cached) {
      return {
        status: cached.release.packages.length > 0 ? 'ready' : 'empty',
        release: cached.release,
        stale: true,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorKind: 'offline',
        errorDetail: 'Network client unavailable in this environment.',
      };
    }
    return {
      status: 'error',
      release: null,
      stale: false,
      fromCache: false,
      errorKind: 'offline',
      errorDetail: 'Network client unavailable.',
    };
  }

  const controller = typeof AbortController === 'function' ? new AbortController() : null;
  const timerId = controller
    ? setTimeout(() => controller.abort(new Error('timeout')), timeoutMs)
    : null;

  try {
    const response = await fetchImpl(apiUrl, {
      method: 'GET',
      headers: {
        Accept: 'application/vnd.github+json',
      },
      signal: controller ? controller.signal : undefined,
      cache: 'no-store',
    });

    if (response.status === 403 || response.status === 429) {
      const resetHeader = response.headers && typeof response.headers.get === 'function'
        ? response.headers.get('X-RateLimit-Reset')
        : null;
      const resetEpoch = resetHeader && /^\d+$/.test(resetHeader) ? Number(resetHeader) * 1000 : null;
      if (cached) {
        return {
          status: cached.release.packages.length > 0 ? 'ready' : 'empty',
          release: cached.release,
          stale: true,
          fromCache: true,
          fetchedAt: cached.fetchedAt,
          errorKind: 'ratelimit',
          rateLimitResetAt: resetEpoch,
          errorDetail: 'GitHub API rate limit reached; showing cached release metadata.',
        };
      }
      return {
        status: 'error',
        release: null,
        stale: false,
        fromCache: false,
        errorKind: 'ratelimit',
        rateLimitResetAt: resetEpoch,
        errorDetail: 'GitHub API rate limit reached for this IP address.',
      };
    }

    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    const raw = await response.json();
    const release = normalizeReleasePayload(raw);
    if (!release) {
      if (cached) {
        return {
          status: cached.release.packages.length > 0 ? 'ready' : 'empty',
          release: cached.release,
          stale: true,
          fromCache: true,
          fetchedAt: cached.fetchedAt,
          errorKind: 'invalid',
          errorDetail: 'GitHub returned incomplete release metadata; showing cached release.',
        };
      }
      return {
        status: 'error',
        release: null,
        stale: false,
        fromCache: false,
        errorKind: 'invalid',
        errorDetail: 'Release metadata was incomplete or marked draft/prerelease.',
      };
    }

    const savedAt = now();
    writeReleaseCache(storage, raw, release, savedAt);

    return {
      status: release.packages.length > 0 ? 'ready' : 'empty',
      release,
      stale: false,
      fromCache: false,
      fetchedAt: savedAt,
      errorKind: null,
      errorDetail: null,
    };
  } catch (err) {
    const isTimeout = Boolean(err && (err.name === 'AbortError' || err.message === 'timeout'));
    const errorKind = isTimeout ? 'timeout' : 'network';
    const errorDetail = isTimeout
      ? 'GitHub API request timed out after 8 seconds.'
      : 'Could not reach GitHub Releases.';

    if (cached) {
      return {
        status: cached.release.packages.length > 0 ? 'ready' : 'empty',
        release: cached.release,
        stale: true,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorKind,
        errorDetail,
      };
    }

    return {
      status: 'error',
      release: null,
      stale: false,
      fromCache: false,
      errorKind,
      errorDetail,
    };
  } finally {
    if (timerId !== null) {
      clearTimeout(timerId);
    }
  }
}
