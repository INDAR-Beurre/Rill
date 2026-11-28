import {
  RELEASES_FALLBACK_URL,
  buildInstallCommands,
  fetchLatestRelease,
  formatPackageOptionLabel,
  getSystemRequirements,
  readReleaseCache,
} from './release-core.mjs';

function syncHeroShotLink(dark) {
  const heroImg = document.getElementById('hero-screenshot');
  const heroShotLink = document.getElementById('hero-shot-link');
  if (!heroImg || !heroShotLink) {
    return;
  }
  const href = dark ? heroImg.dataset.dark : heroImg.dataset.light;
  if (href) {
    heroShotLink.href = href;
  }
}

function initColorSchemeSync() {
  if (!window.matchMedia) {
    return;
  }
  const mq = window.matchMedia('(prefers-color-scheme: dark)');
  syncHeroShotLink(mq.matches);
  if (typeof mq.addEventListener === 'function') {
    mq.addEventListener('change', (ev) => syncHeroShotLink(ev.matches));
  }
}

function formatPublishedDate(isoString) {
  if (!isoString) {
    return '';
  }
  const date = new Date(isoString);
  if (Number.isNaN(date.getTime())) {
    return '';
  }
  return new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(date);
}

function formatShortTime(epochMs) {
  const date = new Date(epochMs);
  if (Number.isNaN(date.getTime())) {
    return '';
  }
  return new Intl.DateTimeFormat(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  }).format(date);
}

function initDownloadWorkbench() {
  if (!document.getElementById('download-workbench')) {
    return;
  }

  const heroSummaryEl = document.getElementById('hero-release-summary');
  const titleEl = document.getElementById('release-title');
  const metaEl = document.getElementById('release-meta');
  const notesLink = document.getElementById('release-notes-link');
  const noticeEl = document.getElementById('release-notice');
  const readyEl = document.getElementById('release-ready');
  const fallbackEl = document.getElementById('release-fallback');
  const fallbackMsgEl = document.getElementById('fallback-message');
  const archLabel = document.getElementById('arch-label');
  const archWrap = document.getElementById('arch-select-wrap');
  const archSelect = document.getElementById('arch-select');
  const archReadout = document.getElementById('arch-readout');
  const pkgLabel = document.getElementById('package-label');
  const pkgWrap = document.getElementById('package-select-wrap');
  const packageSelect = document.getElementById('package-select');
  const pkgReadout = document.getElementById('package-readout');
  const availSummary = document.getElementById('availability-summary');
  const reqHeadline = document.getElementById('requirements-headline');
  const reqDetail = document.getElementById('requirements-detail');
  const downloadLink = document.getElementById('download-link');
  const assetMeta = document.getElementById('asset-meta');
  const sidecarLink = document.getElementById('sidecar-link');
  const sigLink = document.getElementById('sig-link');
  const digestLine = document.getElementById('digest-line');
  const terminalTitle = document.getElementById('terminal-title');
  const installCommands = document.getElementById('install-commands');
  const commandsNote = document.getElementById('commands-note');
  const copyBtn = document.getElementById('copy-commands');
  const copyStatus = document.getElementById('copy-status');
  const retryBtn = document.getElementById('retry-release');

  let currentRelease = null;
  let currentCommandText = '';
  let releaseRequestSeq = 0;
  let copyResetTimer = null;

  function setHeroNeutralLink() {
    if (!heroSummaryEl) {
      return;
    }
    heroSummaryEl.textContent = '';
    const link = document.createElement('a');
    link.className = 'text-link';
    link.href = RELEASES_FALLBACK_URL;
    link.rel = 'noopener noreferrer';
    link.textContent = 'View latest stable release on GitHub';
    heroSummaryEl.appendChild(link);
  }

  function setNotice(message) {
    if (!noticeEl) {
      return;
    }
    if (!message) {
      noticeEl.hidden = true;
      noticeEl.textContent = '';
      return;
    }
    noticeEl.textContent = message;
    noticeEl.hidden = false;
  }

  function syncFieldControl({ count, wrap, readout, label, selectId, singleText }) {
    if (count <= 1) {
      wrap.hidden = true;
      readout.hidden = false;
      readout.textContent = singleText;
      label.removeAttribute('for');
    } else {
      wrap.hidden = false;
      readout.hidden = true;
      label.setAttribute('for', selectId);
    }
  }

  function populateSelectors(release) {
    const arches = [...new Set(release.packages.map((p) => p.arch))];
    const prevArch = archSelect.value;
    archSelect.textContent = '';
    for (const arch of arches) {
      const opt = document.createElement('option');
      opt.value = arch;
      opt.textContent = arch;
      archSelect.appendChild(opt);
    }
    if (arches.includes(prevArch)) {
      archSelect.value = prevArch;
    }
    syncFieldControl({
      count: arches.length,
      wrap: archWrap,
      readout: archReadout,
      label: archLabel,
      selectId: 'arch-select',
      singleText: arches[0] || '',
    });
    syncPackageOptions(release);
  }

  function syncPackageOptions(release) {
    const selectedArch = archSelect.value;
    const matching = release.packages.filter((p) => p.arch === selectedArch);
    const formatCounts = new Map();
    for (const pkg of matching) {
      formatCounts.set(pkg.format, (formatCounts.get(pkg.format) || 0) + 1);
    }

    const prevName = packageSelect.value;
    packageSelect.textContent = '';
    for (const pkg of matching) {
      const opt = document.createElement('option');
      opt.value = pkg.name;
      const hasSibling = (formatCounts.get(pkg.format) || 0) > 1;
      opt.textContent = formatPackageOptionLabel(pkg, hasSibling);
      packageSelect.appendChild(opt);
    }
    if (matching.some((p) => p.name === prevName)) {
      packageSelect.value = prevName;
    }
    const singleLabel = matching[0] ? formatPackageOptionLabel(matching[0], false) : '';
    syncFieldControl({
      count: matching.length,
      wrap: pkgWrap,
      readout: pkgReadout,
      label: pkgLabel,
      selectId: 'package-select',
      singleText: singleLabel,
    });
    renderSelectedPackage(release);
  }

  function renderSelectedPackage(release) {
    const selectedArch = archSelect.value;
    const selectedName = packageSelect.value;
    const pkg =
      release.packages.find((p) => p.name === selectedName && p.arch === selectedArch) ||
      release.packages.find((p) => p.arch === selectedArch) ||
      release.packages[0];

    if (!pkg) {
      return;
    }

    if (release.packages.length === 1) {
      availSummary.textContent = `Publishes ${pkg.arch} ${pkg.format} only.`;
    } else {
      const formatList = [...new Set(release.packages.map((p) => p.format))].join(', ');
      availSummary.textContent = `Available package formats in ${release.title}: ${formatList}.`;
    }

    const reqs = getSystemRequirements(release, pkg);
    reqHeadline.textContent = reqs.headline;
    reqDetail.textContent = reqs.detail;

    if (heroSummaryEl) {
      const glibcNote = release.version.startsWith('1.1.') && pkg.format === '.AppImage'
        ? ', glibc 2.44+'
        : '';
      heroSummaryEl.textContent = `Latest: ${release.title}, ${pkg.arch} ${pkg.format} (${pkg.sizeLabel}${glibcNote})`;
    }

    downloadLink.href = pkg.url;
    downloadLink.textContent = `Download ${pkg.format}`;
    assetMeta.textContent = `${pkg.name} (${pkg.sizeLabel})`;

    if (pkg.sidecar) {
      sidecarLink.href = pkg.sidecar.url;
      sidecarLink.textContent = `Download ${pkg.sidecar.name}`;
      sidecarLink.hidden = false;
    } else {
      sidecarLink.hidden = true;
      sidecarLink.href = RELEASES_FALLBACK_URL;
    }

    if (pkg.sigSidecar) {
      sigLink.href = pkg.sigSidecar.url;
      sigLink.textContent = `External signature: ${pkg.sigSidecar.name} (verify manually with gpg)`;
      sigLink.hidden = false;
    } else {
      sigLink.hidden = true;
      sigLink.href = RELEASES_FALLBACK_URL;
    }

    if (pkg.sha256) {
      digestLine.textContent = `SHA-256: ${pkg.sha256}`;
      digestLine.hidden = false;
    } else {
      digestLine.hidden = true;
      digestLine.textContent = '';
    }

    const cmds = buildInstallCommands(pkg);
    currentCommandText = cmds.lines.join('\n');
    installCommands.textContent = currentCommandText;
    commandsNote.textContent = cmds.note;
    if (terminalTitle) {
      terminalTitle.textContent = cmds.hasVerification
        ? 'Verify checksum and run locally'
        : 'Run locally (no checksum published for this asset)';
    }
  }

  function renderResult(result) {
    if (result.status === 'ready' && result.release) {
      const release = result.release;
      currentRelease = release;
      const dateStr = formatPublishedDate(release.publishedAt);
      titleEl.textContent = release.title;
      metaEl.textContent = dateStr
        ? `Tag ${release.tag}, published ${dateStr}`
        : `Tag ${release.tag}`;

      if (notesLink) {
        notesLink.href = release.htmlUrl || RELEASES_FALLBACK_URL;
        notesLink.hidden = false;
      }

      if (result.stale) {
        const resetNote = result.rateLimitResetAt
          ? ` Rate limit resets around ${formatShortTime(result.rateLimitResetAt)}.`
          : '';
        setNotice(`${result.errorDetail}${resetNote}`);
      } else if (result.fromCache && result.fetchedAt) {
        setNotice(`Loaded cached release metadata from ${formatShortTime(result.fetchedAt)}. Use Refresh to check GitHub now.`);
      } else {
        setNotice('');
      }

      fallbackEl.hidden = true;
      readyEl.hidden = false;
      populateSelectors(release);
      return;
    }

    setHeroNeutralLink();
    readyEl.hidden = true;
    fallbackEl.hidden = false;
    setNotice('');

    if (result.status === 'empty' && result.release) {
      titleEl.textContent = result.release.title;
      metaEl.textContent = `Tag ${result.release.tag}`;
      if (notesLink) {
        notesLink.href = result.release.htmlUrl || RELEASES_FALLBACK_URL;
        notesLink.hidden = false;
      }
      fallbackMsgEl.textContent =
        'This release does not include prebuilt Linux package files (.AppImage, .deb, .rpm, .pkg.tar.zst, or .tar.gz). Check the release notes on GitHub or build from source below.';
      return;
    }

    if (notesLink) {
      notesLink.hidden = true;
    }
    titleEl.textContent = 'Live release lookup unavailable';
    metaEl.textContent = 'Could not verify the latest release from GitHub';

    if (result.errorKind === 'ratelimit') {
      const resetNote = result.rateLimitResetAt
        ? ` GitHub indicates the limit resets around ${formatShortTime(result.rateLimitResetAt)}.`
        : '';
      fallbackMsgEl.textContent = `GitHub API rate limit reached for this connection.${resetNote} Download directly from the official GitHub Releases page.`;
    } else if (result.errorKind === 'timeout') {
      fallbackMsgEl.textContent =
        'The GitHub request timed out after 8 seconds. Retry or open the official GitHub Releases page directly.';
    } else {
      fallbackMsgEl.textContent =
        'Unable to reach GitHub right now (offline or blocked). Open the official GitHub Releases page directly or build from source.';
    }
  }

  async function loadRelease({ bypassCache = false } = {}) {
    const reqSeq = ++releaseRequestSeq;
    const cached = bypassCache ? null : readReleaseCache(window.localStorage);

    if (cached && cached.fresh && cached.release.packages.length > 0) {
      renderResult({
        status: 'ready',
        release: cached.release,
        stale: false,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorKind: null,
        errorDetail: null,
      });
      return;
    }

    if (cached && cached.release.packages.length > 0) {
      renderResult({
        status: 'ready',
        release: cached.release,
        stale: true,
        fromCache: true,
        fetchedAt: cached.fetchedAt,
        errorDetail: `Showing cached metadata from ${formatShortTime(cached.fetchedAt)} while checking GitHub.`,
      });
    } else {
      titleEl.textContent = 'Latest release';
      metaEl.textContent = 'Checking GitHub for the latest stable build';
      setNotice('');
      readyEl.hidden = true;
      fallbackEl.hidden = true;
    }

    const fresh = await fetchLatestRelease({
      storage: window.localStorage,
      bypassCache,
    });
    if (reqSeq !== releaseRequestSeq) {
      return;
    }
    renderResult(fresh);
  }

  archSelect.addEventListener('change', () => {
    if (currentRelease) {
      syncPackageOptions(currentRelease);
    }
  });

  packageSelect.addEventListener('change', () => {
    if (currentRelease) {
      renderSelectedPackage(currentRelease);
    }
  });

  retryBtn.addEventListener('click', () => {
    loadRelease({ bypassCache: true });
  });

  copyBtn.addEventListener('click', async () => {
    if (!currentCommandText) {
      return;
    }
    try {
      await navigator.clipboard.writeText(currentCommandText);
      copyBtn.textContent = 'Copied';
      copyStatus.textContent = 'Commands copied to clipboard.';
      if (copyResetTimer !== null) {
        clearTimeout(copyResetTimer);
      }
      copyResetTimer = setTimeout(() => {
        copyBtn.textContent = 'Copy commands';
        copyResetTimer = null;
      }, 1800);
    } catch {
      copyStatus.textContent = 'Clipboard copy unavailable; select the command text manually.';
    }
  });

  loadRelease();
}

function initScreenshotLightbox() {
  const dialog = document.getElementById('screenshot-lightbox');
  const stage = document.getElementById('lightbox-stage');
  const shotLinks = [...document.querySelectorAll('[data-shot-link]')];

  if (!dialog || typeof dialog.showModal !== 'function' || !stage || shotLinks.length === 0) {
    return;
  }

  const groupEl = document.getElementById('lightbox-group');
  const counterEl = document.getElementById('lightbox-counter');
  const rawLink = document.getElementById('lightbox-raw-link');
  const titleEl = document.getElementById('lightbox-title');
  const descEl = document.getElementById('lightbox-desc');
  const closeBtn = document.getElementById('lightbox-close');
  const prevBtn = document.getElementById('lightbox-prev');
  const nextBtn = document.getElementById('lightbox-next');

  let activeIndex = 0;
  let lastTrigger = null;

  function renderAt(index) {
    const len = shotLinks.length;
    activeIndex = ((index % len) + len) % len;
    const link = shotLinks[activeIndex];
    const thumb = link.querySelector('img');
    const figure = link.closest('.shot-card');
    const section = link.closest('[data-shot-group]');
    const titleNode = figure ? figure.querySelector('.shot-title') : null;
    const descNode = figure ? figure.querySelector('.shot-desc') : null;
    const href = link.getAttribute('href') || '';

    stage.textContent = '';
    const fullImg = document.createElement('img');
    fullImg.className = 'lightbox-img';
    fullImg.src = href;
    fullImg.alt = thumb ? thumb.alt : '';
    if (thumb && thumb.width > 0 && thumb.height > 0) {
      fullImg.width = thumb.width;
      fullImg.height = thumb.height;
    }
    stage.appendChild(fullImg);

    if (groupEl) {
      groupEl.textContent = section?.dataset.shotGroup || 'Rill screenshot';
    }
    if (counterEl) {
      counterEl.textContent = `${activeIndex + 1} / ${len}`;
    }
    if (rawLink) {
      rawLink.href = href;
    }
    if (titleEl) {
      titleEl.textContent = titleNode ? titleNode.textContent.trim() : 'Screenshot';
    }
    if (descEl) {
      descEl.textContent = descNode ? descNode.textContent.trim().replace(/\s+/g, ' ') : '';
    }
    if (prevBtn) {
      prevBtn.disabled = len <= 1;
    }
    if (nextBtn) {
      nextBtn.disabled = len <= 1;
    }
  }

  for (let i = 0; i < shotLinks.length; i += 1) {
    const link = shotLinks[i];
    link.addEventListener('click', (ev) => {
      if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey || ev.button !== 0) {
        return;
      }
      ev.preventDefault();
      lastTrigger = link;
      renderAt(i);
      if (!dialog.open) {
        dialog.showModal();
      }
    });
  }

  if (closeBtn) {
    closeBtn.addEventListener('click', () => dialog.close());
  }
  if (prevBtn) {
    prevBtn.addEventListener('click', () => renderAt(activeIndex - 1));
  }
  if (nextBtn) {
    nextBtn.addEventListener('click', () => renderAt(activeIndex + 1));
  }

  dialog.addEventListener('click', (ev) => {
    if (ev.target === dialog) {
      dialog.close();
    }
  });

  dialog.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowLeft') {
      ev.preventDefault();
      renderAt(activeIndex - 1);
    } else if (ev.key === 'ArrowRight') {
      ev.preventDefault();
      renderAt(activeIndex + 1);
    }
  });

  dialog.addEventListener('close', () => {
    stage.textContent = '';
    if (lastTrigger && typeof lastTrigger.focus === 'function') {
      lastTrigger.focus();
    }
    lastTrigger = null;
  });
}

function init() {
  initColorSchemeSync();
  initDownloadWorkbench();
  initScreenshotLightbox();
}

init();
