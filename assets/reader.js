// Reading mode, done in the page inside an isolated WebKit script world
// ("search-reader"): take the article out of the page it is in and set it
// again in the browser's own type. No second view, no extra process, Back
// still works because the address never changes, and page scripts in the main
// world cannot see or tamper with reader state or saved DOM nodes.
(() => {
  if (globalThis.__searchReaderCtrl) return;

  const XHTML_NS = 'http://www.w3.org/1999/xhtml';
  const DROP_TAGS = new Set([
    'script', 'style', 'noscript', 'template', 'iframe', 'frame', 'frameset',
    'object', 'embed', 'applet', 'svg', 'math', 'meta', 'base', 'link',
    'title', 'head', 'form', 'input', 'textarea', 'select', 'option',
    'optgroup', 'button', 'fieldset', 'legend', 'datalist', 'output',
    'portal', 'canvas', 'dialog', 'slot', 'marquee',
    'nav', 'aside', 'footer', 'header',
  ]);
  const JUNK_CLASS_PARTS = ['share', 'comment', 'newsletter', 'cookie', 'sidebar', 'related'];
  const GLOBAL_SAFE_ATTRS = new Set(['dir', 'lang', 'title']);
  const TAG_SAFE_ATTRS = {
    a: new Set(['href']),
    img: new Set(['src', 'alt', 'width', 'height', 'loading', 'decoding']),
    video: new Set(['src', 'controls', 'width', 'height']),
    audio: new Set(['src', 'controls']),
    source: new Set(['src', 'type']),
    td: new Set(['colspan', 'rowspan', 'headers', 'scope']),
    th: new Set(['colspan', 'rowspan', 'headers', 'scope']),
    ol: new Set(['start', 'reversed', 'type']),
    time: new Set(['datetime']),
    details: new Set(['open']),
    blockquote: new Set(['cite']),
    q: new Set(['cite']),
  };
  const RASTER_DATA_URL = /^data:image\/(png|jpeg|jpg|gif|webp);base64,[a-z0-9+/=]+$/i;

  let active = false;
  let savedFragment = null;
  let savedStyles = null;
  let savedScroll = 0;
  let fontSize = 18;

  const isSafeUrl = (raw, tag, attr) => {
    const value = String(raw || '').trim();
    if (!value) return false;
    const clean = value.replace(/[\u0000-\u0020\u007f]+/g, '').toLowerCase();
    if (!clean) return false;
    if (
      clean.startsWith('javascript:') ||
      clean.startsWith('vbscript:') ||
      clean.startsWith('file:') ||
      clean.startsWith('blob:') ||
      clean.startsWith('filesystem:')
    ) {
      return false;
    }
    if (clean.startsWith('data:')) {
      return tag === 'img' && attr === 'src' && RASTER_DATA_URL.test(clean);
    }
    const colon = clean.indexOf(':');
    const boundary = clean.search(/[/?#]/);
    const hasScheme = colon !== -1 && (boundary === -1 || colon < boundary);
    if (hasScheme) {
      if (attr === 'href') {
        return clean.startsWith('https:') || clean.startsWith('http:') || clean.startsWith('mailto:');
      }
      if (attr === 'src' || attr === 'cite') {
        return clean.startsWith('https:') || clean.startsWith('http:');
      }
      return false;
    }
    return true;
  };

  const isJunkElement = (el) => {
    if (el.getAttribute && el.getAttribute('aria-hidden') === 'true') return true;
    const cls = el.getAttribute ? String(el.getAttribute('class') || '').toLowerCase() : '';
    if (!cls) return false;
    for (const part of JUNK_CLASS_PARTS) {
      if (cls.includes(part)) return true;
    }
    return false;
  };

  const sanitizeAttributes = (el, tag) => {
    if (!el.attributes) return;
    const tagAllowed = TAG_SAFE_ATTRS[tag];
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      if (name.startsWith('on') || name === 'style') {
        el.removeAttribute(attr.name);
        continue;
      }
      if (name === 'id') {
        const val = String(attr.value || '').trim();
        if (!val || /^__search/i.test(val)) {
          el.removeAttribute(attr.name);
        }
        continue;
      }
      if (GLOBAL_SAFE_ATTRS.has(name)) {
        continue;
      }
      if (tagAllowed && tagAllowed.has(name)) {
        if (name === 'href' || name === 'src' || name === 'cite') {
          if (!isSafeUrl(attr.value, tag, name)) {
            el.removeAttribute(attr.name);
          }
        }
        continue;
      }
      el.removeAttribute(attr.name);
    }
  };

  const sanitizeTree = (root) => {
    const children = Array.from(root.childNodes);
    for (const child of children) {
      if (child.nodeType === 3) continue; // TEXT_NODE
      if (child.nodeType !== 1) {
        if (child.parentNode) child.parentNode.removeChild(child);
        continue;
      }
      const ns = child.namespaceURI;
      const tag = child.tagName ? child.tagName.toLowerCase() : '';
      if ((ns && ns !== XHTML_NS) || DROP_TAGS.has(tag) || isJunkElement(child)) {
        if (child.parentNode) child.parentNode.removeChild(child);
        continue;
      }
      sanitizeAttributes(child, tag);
      sanitizeTree(child);
    }
  };

  const clearChildren = (node) => {
    while (node.firstChild) {
      node.removeChild(node.firstChild);
    }
  };

  const setFont = (delta) => {
    const current = Number(fontSize) || 18;
    const next = Math.max(14, Math.min(22, current + delta));
    fontSize = next;
    document.documentElement.style.setProperty('--search-reader-size', next + 'px');
    const label = document.getElementById('__search_reader_size_label');
    if (label) label.textContent = next + 'px';
    const less = document.querySelector('#__search_reader_font button[aria-label="Smaller reading text"]');
    const more = document.querySelector('#__search_reader_font button[aria-label="Larger reading text"]');
    if (less) less.disabled = next <= 14;
    if (more) more.disabled = next >= 22;
    return String(next);
  };

  const leave = () => {
    if (!savedFragment || !document.body) {
      active = true;
      return 'failed';
    }
    active = false;
    const styles = savedStyles || [null, null];
    const pageY = Number(savedScroll) || 0;
    try {
      clearChildren(document.body);
      document.body.appendChild(savedFragment);
      savedFragment = null;
    } catch (_) {
      active = true;
      return 'failed';
    }
    const styleHolder = document.head || document.documentElement;
    if (styleHolder) {
      styleHolder.querySelectorAll('style[data-search-reader]').forEach((n) => n.remove());
    }
    [[document.body, styles[0]], [document.documentElement, styles[1]]].forEach(([node, value]) => {
      if (!node) return;
      if (value === null) node.removeAttribute('style');
      else node.setAttribute('style', value);
    });
    savedStyles = null;
    savedScroll = 0;
    window.scrollTo(0, pageY);
    if (typeof requestAnimationFrame === 'function') {
      requestAnimationFrame(() => window.scrollTo(0, pageY));
    }
    if (typeof window.addEventListener === 'function') {
      window.addEventListener('load', () => window.scrollTo(0, pageY), { once: true });
    }
    return 'left';
  };

  const toggle = (palette) => {
    if (active) {
      return leave();
    }
    if (!document.body) return 'empty';

    const candidates = [
      'article',
      '[role=main]',
      'main',
      '.post-content',
      '.entry-content',
      '.article-body',
      '.story-body',
      '#content',
    ];
    let best = null;
    let bestScore = 0;
    for (const selector of candidates) {
      for (const node of document.querySelectorAll(selector)) {
        const text = (node.innerText || node.textContent || '').trim();
        if (text.length > bestScore) {
          best = node;
          bestScore = text.length;
        }
      }
    }
    if (!best || bestScore < 400) best = document.body;
    if (!best) return 'empty';

    const headingNode = document.querySelector('h1');
    const heading =
      (headingNode ? headingNode.innerText || headingNode.textContent : '') ||
      document.title ||
      '';
    const bylineNode = document.querySelector('[rel=author], .byline, .author, [class*=byline]');
    const byline = bylineNode ? (bylineNode.innerText || bylineNode.textContent || '').trim().slice(0, 140) : '';

    const clone = best.cloneNode(true);
    sanitizeTree(clone);

    savedScroll = Number(window.scrollY) || 0;
    savedStyles = [
      document.body.getAttribute('style'),
      document.documentElement.getAttribute('style'),
    ];
    // Move original live DOM nodes into an isolated-world DocumentFragment so
    // restoration never re-parses serialized HTML markup and page scripts cannot
    // tamper with saved markup.
    const fragment = document.createDocumentFragment();
    while (document.body.firstChild) {
      fragment.appendChild(document.body.firstChild);
    }
    savedFragment = fragment;

    // Palette owns the pairs; keeping a second set here loses their contrast.
    const tokens = palette;
    const pageCss = tokens.palette;
    const rule = tokens.rule;
    const muted = tokens.muted;
    const ground = tokens.ground;
    const paper = tokens.paper || rule;

    const styleHolder = document.head || document.documentElement;
    if (styleHolder) {
      styleHolder.querySelectorAll('[data-search-reader]').forEach((n) => n.remove());
      const style = document.createElement('style');
      style.setAttribute('data-search-reader', '');
      style.textContent = `
    html, body { ${pageCss} !important; }
    body { margin: 0 auto !important; padding: 0 !important; max-width: 720px !important; font: var(--search-reader-size, 18px)/1.62 Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif !important;
      -webkit-font-smoothing: antialiased; }
    #__search_reader { box-sizing: border-box; max-width: 660px; margin: 0 auto; padding: 76px 28px 120px; }
    #__search_reader_bar { position: fixed; top: 0; left: 0; right: 0; height: 46px; background: ${ground}; border-bottom: 1px solid ${rule}; z-index: 2147483647; display: flex; align-items: center; justify-content: center; }
    #__search_reader_bar_inner { box-sizing: border-box; width: 100%; max-width: 660px; padding: 0 28px; display: flex; align-items: center; justify-content: space-between; gap: 12px; }
    #__search_reader h1 { font-size: 1.79em; line-height: 1.18; letter-spacing: -0.012em; font-weight: 700; margin: 0 0 12px; }
    #__search_reader .byline { color: ${muted}; font-size: 0.76em; margin: 0 0 40px; }
    #__search_reader p, #__search_reader li, #__search_reader blockquote { margin: 0 0 1.05em; }
    #__search_reader h2, #__search_reader h3 { line-height: 1.25; margin: 1.8em 0 0.5em; letter-spacing: -0.01em; }
    #__search_reader h2 { font-size: 1.32em; font-weight: 600; }
    #__search_reader h3 { font-size: 1.11em; font-weight: 600; letter-spacing: -0.005em; }
    #__search_reader h4, #__search_reader h5, #__search_reader h6 { font-size: 1em; font-weight: 600; line-height: 1.3; margin: 1.6em 0 0.4em; }
    #__search_reader img, #__search_reader video, #__search_reader figure { max-width: 100%; height: auto; border-radius: 6px; }
    #__search_reader a { color: inherit; text-decoration: underline; text-underline-offset: 3px; text-decoration-color: ${rule}; overflow-wrap: anywhere; }
    #__search_reader blockquote { border-left: 2px solid ${rule}; padding-left: 18px; color: ${muted}; }
    #__search_reader code { font-family: "SF Mono", "DejaVu Sans Mono", monospace; font-size: 0.86em; background: ${paper}; border-radius: 4px; padding: 0.12em 0.36em; -webkit-box-decoration-break: clone; box-decoration-break: clone; overflow-wrap: anywhere; }
    #__search_reader pre { font: 0.76em/1.5 "SF Mono", "DejaVu Sans Mono", monospace; background: ${paper}; border-radius: 6px; padding: 14px; white-space: pre-wrap; overflow-wrap: anywhere; overflow-x: auto; }
    #__search_reader pre code { font-size: 1em; background: none; padding: 0; border-radius: 0; white-space: inherit; overflow-wrap: inherit; }
    #__search_reader hr { border: none; border-top: 1px solid ${rule}; margin: 2.4em 0; }
    #__search_reader .search-reader-origin { font-size: max(12px, 0.68em); color: ${muted}; margin-bottom: 28px; }
    #__search_reader_exit:hover { border-color: ${muted} !important; }
    #__search_reader_exit:focus-visible { outline: 2px solid ${muted} !important; outline-offset: 2px !important; border-color: ${muted} !important; }
    #__search_reader_font { display: flex; align-items: center; gap: 6px; }
    #__search_reader_size_label { min-width: 36px; color: ${muted}; font: 500 12px/18px Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif; text-align: center; }
    #__search_reader_font button { all: unset; box-sizing: border-box; min-width: 32px; min-height: 28px; cursor: pointer; text-align: center;
      font: 500 13px/18px Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif;
      color: ${muted}; border: 1px solid ${rule}; border-radius: 8px; background: ${ground}; }
    #__search_reader_font button:hover:not(:disabled) { border-color: ${muted} !important; }
    #__search_reader_font button:disabled { opacity: 0.35; cursor: default; }
    #__search_reader_font button:focus-visible { outline: 2px solid ${muted} !important; outline-offset: 2px !important; }
    #__search_reader ::selection, html::selection, body::selection { background: #f2c94c !important; color: #1b1a17 !important; }
  `;
      styleHolder.appendChild(style);
    }

    const container = document.createElement('div');
    container.id = '__search_reader';
    const exit = document.createElement('button');
    exit.id = '__search_reader_exit';
    exit.type = 'button';
    exit.textContent = 'Leave reading mode';
    exit.setAttribute('style', `all: unset; display: inline-block; box-sizing: border-box; min-height: 28px; cursor: pointer;
    font: 500 13px/18px Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif;
    color: ${muted}; border: 1px solid ${rule}; border-radius: 8px; padding: 4px 12px;
    background: ${ground};`);
    exit.addEventListener('click', () => {
      const answer = toggle(tokens);
      if (answer !== 'left') return;
      try {
        if (window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.searchReader) {
          window.webkit.messageHandlers.searchReader.postMessage('off');
        }
      } catch (_) {}
    });

    const bar = document.createElement('div');
    bar.id = '__search_reader_bar';
    const barInner = document.createElement('div');
    barInner.id = '__search_reader_bar_inner';
    const font = document.createElement('div');
    font.id = '__search_reader_font';
    const makeFontBtn = (label, delta, name) => {
      const button = document.createElement('button');
      button.type = 'button';
      button.textContent = label;
      button.title = name;
      button.setAttribute('aria-label', name);
      button.addEventListener('click', () => setFont(delta));
      return button;
    };
    const sizeLabel = document.createElement('span');
    sizeLabel.id = '__search_reader_size_label';
    sizeLabel.setAttribute('aria-live', 'polite');
    sizeLabel.textContent = '18px';
    const [lessSpec, moreSpec] = [['A−', -4, 'Smaller reading text'], ['A+', 4, 'Larger reading text']];
    font.appendChild(makeFontBtn(...lessSpec));
    font.appendChild(sizeLabel);
    font.appendChild(makeFontBtn(...moreSpec));
    barInner.appendChild(font);
    barInner.appendChild(exit);
    bar.appendChild(barInner);
    container.appendChild(bar);

    const title = document.createElement('h1');
    title.textContent = heading.trim();
    container.appendChild(title);
    if (byline) {
      const line = document.createElement('p');
      line.className = 'byline';
      line.textContent = byline;
      container.appendChild(line);
    }
    const origin = document.createElement('div');
    origin.className = 'search-reader-origin';
    origin.textContent = location.host;
    container.appendChild(origin);

    // Adopt sanitized DOM nodes directly instead of re-parsing serialized HTML markup.
    const body = document.createElement('div');
    while (clone.firstChild) {
      body.appendChild(clone.firstChild);
    }
    container.appendChild(body);

    document.body.appendChild(container);
    setFont(0);
    document.body.scrollTop = 0;
    window.scrollTo(0, 0);
    active = true;
    return 'entered';
  };

  Object.defineProperty(globalThis, '__searchReaderCtrl', {
    value: Object.freeze({
      toggle,
      setFont,
      isActive: () => active,
    }),
    writable: false,
    configurable: false,
    enumerable: false,
  });
})();
