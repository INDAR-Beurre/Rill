import {
  RELEASES_FALLBACK_URL,
  buildInstallCommands,
  fetchLatestRelease,
  formatPackageOptionLabel,
  getSystemRequirements,
  readReleaseCache,
} from './release-core.mjs';

const BAYER_8X8 = [
  [0, 48, 12, 60, 3, 51, 15, 63],
  [32, 16, 44, 28, 35, 19, 47, 31],
  [8, 56, 4, 52, 11, 59, 7, 55],
  [40, 24, 36, 20, 43, 27, 39, 23],
  [2, 50, 14, 62, 1, 49, 13, 61],
  [34, 18, 46, 30, 33, 17, 45, 29],
  [10, 58, 6, 54, 9, 57, 5, 53],
  [42, 26, 38, 22, 41, 25, 37, 21],
];

// Every Showcase view uses a genuine matched Light + Dark GTK 3 capture of the exact same screen,
// distinct from the Hero's Vertical Sidebar & Spaces capture.
const SHOWCASE_VIEWS = {
  'top-tabs': {
    title: 'Top tabs and browser menu on a real page',
    desc: 'Switch between the vertical sidebar and a compact horizontal tab bar anytime from Settings or setup without changing your open Spaces.',
    lightSrc: 'assets/menu-page.png',
    darkSrc: 'assets/top-tabs-dark.png',
    lightCaption: 'Top tabs and browser menu on Wikipedia in Warm paper.',
    darkCaption: 'Top tabs and start page in Warm charcoal.',
    alt: 'Rill browser showing horizontal top tabs, open page menu, and controls',
  },
  shortcuts: {
    title: 'Opt-in search suggestions and starter shortcuts',
    desc: 'Step 3 of setup puts search suggestions and optional starter sites under explicit toggles before your first session begins.',
    lightSrc: 'assets/setup-shortcuts-light.png',
    darkSrc: 'assets/setup-shortcuts-dark.png',
    lightCaption: 'Setup Step 3, Privacy & shortcuts, in Warm paper.',
    darkCaption: 'Setup Step 3, Privacy & shortcuts, in Warm charcoal.',
    alt: 'Rill setup step showing search suggestions toggle and starter site options',
  },
  welcome: {
    title: 'Onboarding inside a real tab',
    desc: 'Configure your look, tab layout, and search engine right in the browser tab or skip straight in with one click and no modal.',
    lightSrc: 'assets/introduction-light.png',
    darkSrc: 'assets/introduction-dark.png',
    lightCaption: 'Welcome tab in Warm paper.',
    darkCaption: 'Welcome tab in Warm charcoal.',
    alt: 'Rill welcome tab with Configure and Use defaults buttons',
  },
};

function currentTheme() {
  return document.documentElement.getAttribute('data-theme') === 'dark' ? 'dark' : 'light';
}

function smoothstep(edge0, edge1, x) {
  const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0)));
  return t * t * (3 - 2 * t);
}

// Flowing Bayer ordered-dither river contour behind the hero window, echoing Rill's welcome wave
// while keeping the headline, summary, release link, and header cleanly on solid ground.
function renderRiverCanvas() {
  const canvas = document.getElementById('rill-river-canvas');
  if (!canvas) {
    return;
  }
  const rect = canvas.getBoundingClientRect();
  const width = Math.max(1, Math.round(rect.width));
  const height = Math.max(1, Math.round(rect.height));
  if (width < 640) {
    return;
  }

  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const pixelWidth = Math.round(width * dpr);
  const pixelHeight = Math.round(height * dpr);
  const cell = 3;
  const cols = Math.ceil(width / cell);
  const rows = Math.ceil(height / cell);

  if (canvas.width !== pixelWidth || canvas.height !== pixelHeight) {
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
  }

  const ctx = canvas.getContext('2d');
  if (!ctx) {
    return;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);

  const dark = currentTheme() === 'dark';
  const bankColor = dark ? 'rgba(66, 58, 50, 0.85)' : 'rgba(212, 208, 198, 0.88)';
  const coreColor = dark ? 'rgba(176, 102, 72, 0.76)' : 'rgba(204, 120, 88, 0.68)';
  const startY = 0.14;
  const endY = 0.97;

  for (let row = 0; row < rows; row += 1) {
    const ny = row / Math.max(1, rows - 1);
    if (ny <= startY || ny >= endY) {
      continue;
    }
    const topFade = smoothstep(startY, 0.25, ny);
    const bottomFade = 1 - smoothstep(0.80, endY, ny);
    const spineX = 0.85
      - 0.69 * smoothstep(0.18, 0.92, ny)
      + 0.075 * Math.sin((ny - 0.18) * Math.PI * 2.1);
    const riverHalfWidth = 0.21 + 0.04 * Math.sin(ny * Math.PI);
    const by = row & 7;

    for (let col = 0; col < cols; col += 1) {
      const nx = col / Math.max(1, cols - 1);
      const dist = Math.abs(nx - spineX);
      if (dist > riverHalfWidth * 1.45) {
        continue;
      }
      const normDist = dist / riverHalfWidth;
      const channelProfile = Math.exp(-normDist * normDist * 1.65);
      const contourBand = 0.84 + 0.16 * Math.cos(normDist * Math.PI * 3.5);
      const centerTextMask = (1 - smoothstep(0.18, 0.34, ny)) * (1 - smoothstep(0.24, 0.36, Math.abs(nx - 0.5)));
      const edgeFade = smoothstep(0.0, 0.04, nx) * (1 - smoothstep(0.96, 1.0, nx));
      const density = channelProfile * contourBand * topFade * bottomFade * edgeFade * (1 - centerTextMask) * 0.42;
      const threshold = (BAYER_8X8[by][col & 7] + 0.5) / 64;
      if (density > threshold) {
        const accentRatio = Math.max(0, Math.min(1, 1 - normDist * 1.05));
        const accentThreshold = (BAYER_8X8[(by + 3) & 7][(col + 5) & 7] + 0.5) / 64;
        ctx.fillStyle = accentRatio > accentThreshold ? coreColor : bankColor;
        ctx.fillRect(col * cell, row * cell, 2, 2);
      }
    }
  }
}

let activeViewKey = 'welcome';

function updateThemeUI() {
  const dark = currentTheme() === 'dark';
  const meta = document.getElementById('theme-color-scheme');
  if (meta) {
    meta.setAttribute('content', dark ? 'dark' : 'light');
  }
  const toggleBtn = document.getElementById('theme-toggle');
  if (toggleBtn) {
    toggleBtn.setAttribute('aria-pressed', dark ? 'true' : 'false');
    toggleBtn.setAttribute(
      'aria-label',
      dark ? 'Switch to Warm paper light theme' : 'Switch to Warm charcoal dark theme',
    );
    for (const opt of toggleBtn.querySelectorAll('.theme-option')) {
      const isMatch = opt.dataset.themeOpt === (dark ? 'dark' : 'light');
      opt.setAttribute('data-active', isMatch ? 'true' : 'false');
    }
  }

  const heroImg = document.getElementById('hero-screenshot');
  const heroCaption = document.getElementById('hero-caption');
  if (heroImg) {
    const src = dark ? heroImg.dataset.dark : heroImg.dataset.light;
    if (src) {
      heroImg.src = src;
    }
  }
  if (heroCaption) {
    heroCaption.textContent = dark
      ? 'Vertical tabs and Spaces in Warm charcoal.'
      : 'Spaces and vertical tabs in Warm paper.';
  }

  renderShowcaseView(activeViewKey);
  renderRiverCanvas();
}

function renderShowcaseView(viewKey) {
  const view = SHOWCASE_VIEWS[viewKey] || SHOWCASE_VIEWS.welcome;
  activeViewKey = SHOWCASE_VIEWS[viewKey] ? viewKey : 'welcome';
  const dark = currentTheme() === 'dark';

  let activeTabId = 'tab-welcome';
  for (const btn of document.querySelectorAll('.view-tab')) {
    const selected = btn.dataset.view === activeViewKey;
    btn.setAttribute('aria-selected', selected ? 'true' : 'false');
    btn.setAttribute('tabindex', selected ? '0' : '-1');
    if (selected && btn.id) {
      activeTabId = btn.id;
    }
  }

  const panel = document.getElementById('showcase-tabpanel');
  if (panel) {
    panel.setAttribute('aria-labelledby', activeTabId);
  }

  const img = document.getElementById('showcase-image');
  const title = document.getElementById('showcase-view-title');
  const desc = document.getElementById('showcase-view-desc');
  const caption = document.getElementById('showcase-caption');

  if (img) {
    img.src = dark ? view.darkSrc : view.lightSrc;
    img.alt = view.alt;
  }
  if (title) {
    title.textContent = view.title;
  }
  if (desc) {
    desc.textContent = view.desc;
  }
  if (caption) {
    caption.textContent = dark ? view.darkCaption : view.lightCaption;
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

  function setHeroNeutralLink() {
    if (!heroSummaryEl) {
      return;
    }
    heroSummaryEl.textContent = '';
    const link = document.createElement('a');
    link.className = 'link-quiet';
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
      heroSummaryEl.textContent = `Latest stable: ${release.title}, ${pkg.arch} ${pkg.format} (${pkg.sizeLabel}${glibcNote})`;
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
        setNotice(`Loaded cached release metadata from ${formatShortTime(result.fetchedAt)}. Use Refresh release to check GitHub now.`);
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
      setTimeout(() => {
        copyBtn.textContent = 'Copy commands';
      }, 1800);
    } catch {
      copyStatus.textContent = 'Clipboard copy unavailable; select the command text manually.';
    }
  });

  loadRelease();
}

function initGallery() {
  const filterLinks = [...document.querySelectorAll('.gallery-filter')];
  const groups = [...document.querySelectorAll('.gallery-group')];
  const statusEl = document.getElementById('gallery-status');
  const shotLinks = [...document.querySelectorAll('[data-gallery-shot]')];

  function applyFilter(filterKey) {
    let firstVisibleAssigned = false;
    let visibleShots = 0;
    let matchedGroupLabel = 'all categories';

    for (const group of groups) {
      const show = filterKey === 'all' || group.dataset.group === filterKey;
      group.hidden = !show;
      group.classList.remove('is-first-visible');
      if (show) {
        if (!firstVisibleAssigned) {
          group.classList.add('is-first-visible');
          firstVisibleAssigned = true;
        }
        visibleShots += group.querySelectorAll('[data-gallery-shot]').length;
        if (filterKey !== 'all' && group.dataset.groupLabel) {
          matchedGroupLabel = group.dataset.groupLabel;
        }
      }
    }

    for (const link of filterLinks) {
      const isMatch = link.dataset.filter === filterKey;
      if (isMatch) {
        link.setAttribute('aria-current', 'true');
      } else {
        link.removeAttribute('aria-current');
      }
    }

    if (statusEl) {
      statusEl.textContent = filterKey === 'all'
        ? `Showing all ${visibleShots} screenshots.`
        : `Showing ${visibleShots} screenshots in ${matchedGroupLabel}.`;
    }
  }

  for (const link of filterLinks) {
    link.addEventListener('click', (ev) => {
      if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey || ev.button !== 0) {
        return;
      }
      ev.preventDefault();
      applyFilter(link.dataset.filter || 'all');
    });
  }

  const dialog = document.getElementById('gallery-lightbox');
  const stage = document.getElementById('lightbox-stage');
  const groupEl = document.getElementById('lightbox-group');
  const counterEl = document.getElementById('lightbox-counter');
  const rawLink = document.getElementById('lightbox-raw-link');
  const titleEl = document.getElementById('lightbox-title');
  const descEl = document.getElementById('lightbox-desc');
  const closeBtn = document.getElementById('lightbox-close');
  const prevBtn = document.getElementById('lightbox-prev');
  const nextBtn = document.getElementById('lightbox-next');

  if (!dialog || typeof dialog.showModal !== 'function' || !stage) {
    return;
  }

  let activeShots = [];
  let activeIndex = 0;
  let lastTrigger = null;

  function getVisibleShots() {
    return shotLinks.filter((link) => {
      const group = link.closest('.gallery-group');
      return !group || !group.hidden;
    });
  }

  function renderLightboxAt(index) {
    if (activeShots.length === 0) {
      return;
    }
    const len = activeShots.length;
    activeIndex = ((index % len) + len) % len;
    const link = activeShots[activeIndex];
    const thumb = link.querySelector('img');
    const figure = link.closest('.gallery-figure');
    const group = link.closest('.gallery-group');
    const titleNode = figure ? figure.querySelector('.gallery-caption-title') : null;
    const descNode = figure ? figure.querySelector('.gallery-caption-desc') : null;
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
      groupEl.textContent = group?.dataset.groupLabel || 'Rill screenshot';
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

  function openLightbox(triggerLink) {
    activeShots = getVisibleShots();
    const idx = activeShots.indexOf(triggerLink);
    if (idx === -1) {
      return;
    }
    lastTrigger = triggerLink;
    renderLightboxAt(idx);
    if (!dialog.open) {
      dialog.showModal();
    }
  }

  for (const link of shotLinks) {
    link.addEventListener('click', (ev) => {
      if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey || ev.button !== 0) {
        return;
      }
      ev.preventDefault();
      openLightbox(link);
    });
  }

  if (closeBtn) {
    closeBtn.addEventListener('click', () => {
      dialog.close();
    });
  }

  if (prevBtn) {
    prevBtn.addEventListener('click', () => {
      renderLightboxAt(activeIndex - 1);
    });
  }

  if (nextBtn) {
    nextBtn.addEventListener('click', () => {
      renderLightboxAt(activeIndex + 1);
    });
  }

  dialog.addEventListener('click', (ev) => {
    if (ev.target === dialog) {
      dialog.close();
    }
  });

  dialog.addEventListener('keydown', (ev) => {
    if (ev.key === 'ArrowLeft') {
      ev.preventDefault();
      renderLightboxAt(activeIndex - 1);
    } else if (ev.key === 'ArrowRight') {
      ev.preventDefault();
      renderLightboxAt(activeIndex + 1);
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
  const toggleBtn = document.getElementById('theme-toggle');
  if (toggleBtn) {
    toggleBtn.addEventListener('click', () => {
      const next = currentTheme() === 'dark' ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', next);
      try {
        localStorage.setItem('rill.site.theme', next);
      } catch {
        // Ignore private browsing storage error.
      }
      updateThemeUI();
    });
  }

  const tabButtons = [...document.querySelectorAll('.view-tab')];
  tabButtons.forEach((btn, idx) => {
    btn.addEventListener('click', () => {
      renderShowcaseView(btn.dataset.view);
    });
    btn.addEventListener('keydown', (ev) => {
      let nextIdx = null;
      if (ev.key === 'ArrowRight') {
        nextIdx = (idx + 1) % tabButtons.length;
      } else if (ev.key === 'ArrowLeft') {
        nextIdx = (idx - 1 + tabButtons.length) % tabButtons.length;
      } else if (ev.key === 'Home') {
        nextIdx = 0;
      } else if (ev.key === 'End') {
        nextIdx = tabButtons.length - 1;
      }
      if (nextIdx !== null) {
        ev.preventDefault();
        tabButtons[nextIdx].focus();
        renderShowcaseView(tabButtons[nextIdx].dataset.view);
      }
    });
  });

  let resizeTimer = null;
  window.addEventListener('resize', () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(renderRiverCanvas, 80);
  });

  updateThemeUI();
  initGallery();
  initDownloadWorkbench();
}

init();
