// Reading mode, done in the page: take the article out of the page it is in
// and set it again in the browser's own type. No second view, no extra
// process, and Back still works because the address never changes.
window.__searchReader = function (palette) {
  if (window.__searchReaderOn) {
    window.__searchReaderOn = false;
    if (window.__searchReaderRestore) {
      try {
        eval(window.__searchReaderRestore);
      } catch (_) {}
      window.__searchReaderRestore = null;
    }
    return true;
  }

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
      const text = (node.innerText || '').trim();
      if (text.length > bestScore) {
        best = node;
        bestScore = text.length;
      }
    }
  }
  if (!best || bestScore < 400) best = document.body;
  if (!best) return false;

  const clone = best.cloneNode(true);
  const junk = 'script, style, nav, aside, form, iframe, noscript, footer, header, button, [class*=share], [class*=comment], [class*=newsletter], [class*=cookie], [class*=sidebar], [class*=related], [aria-hidden=true]';
  clone.querySelectorAll(junk).forEach((n) => n.remove());

  const heading =
    (document.querySelector('h1') || {}).innerText ||
    document.title ||
    '';
  const bylineNode = document.querySelector('[rel=author], .byline, .author, [class*=byline]');
  const byline = bylineNode ? (bylineNode.innerText || '').trim().slice(0, 140) : '';

  const oldBody = document.body.innerHTML;
  window.__searchReaderRestore = 'document.body.innerHTML = window.__searchReaderBody;';
  window.__searchReaderBody = oldBody;

  const dark = /#0B0B0B/i.test(palette);
  const rule = dark ? '#242424' : '#EAEAEA';
  const muted = dark ? '#8A8A8A' : '#6B6B6B';
  const ground = dark ? '#0B0B0B' : '#FFFFFF';

  document.head.querySelectorAll('[data-search-reader]').forEach((n) => n.remove());
  const style = document.createElement('style');
  style.setAttribute('data-search-reader', '');
  style.textContent = `
    html, body { ${palette} !important; }
    body { margin: 0 auto !important; max-width: 720px !important; font: 19px/1.62 "SF Pro Text", Inter, "Adwaita Sans", "Noto Sans", sans-serif !important;
      -webkit-font-smoothing: antialiased; }
    #__search_reader { max-width: 660px; margin: 0 auto; padding: 72px 28px 120px; }
    #__search_reader h1 { font-size: 34px; line-height: 1.18; letter-spacing: -0.012em; font-weight: 700; margin: 0 0 12px; }
    #__search_reader .byline { color: ${muted}; font-size: 13px; margin: 0 0 40px; }
    #__search_reader p, #__search_reader li, #__search_reader blockquote { margin: 0 0 1.05em; }
    #__search_reader h2, #__search_reader h3 { line-height: 1.25; margin: 1.8em 0 0.5em; letter-spacing: -0.01em; }
    #__search_reader img, #__search_reader video, #__search_reader figure { max-width: 100%; height: auto; border-radius: 6px; }
    #__search_reader a { color: inherit; text-decoration: underline; text-underline-offset: 3px; text-decoration-color: ${rule}; }
    #__search_reader blockquote { border-left: 2px solid ${rule}; padding-left: 18px; color: ${muted}; }
    #__search_reader pre, #__search_reader code { font: 14px/1.5 "SF Mono", "DejaVu Sans Mono", monospace; background: ${rule}; border-radius: 6px; }
    #__search_reader pre { padding: 14px; overflow-x: auto; }
    #__search_reader hr { border: none; border-top: 1px solid ${rule}; margin: 2.4em 0; }
    #__search_reader .search-reader-origin { font-size: 12px; color: ${muted}; margin-bottom: 28px; }
  `;
  document.head.appendChild(style);

  const container = document.createElement('div');
  container.id = '__search_reader';
  const exit = document.createElement('button');
  exit.id = '__search_reader_exit';
  exit.type = 'button';
  exit.textContent = 'Leave reading mode';
  // Stays where it can be reached: an article can be thousands of pixels long,
  // and a way out that scrolls away is not a way out.
  exit.setAttribute('style', `all: unset; display: inline-block; cursor: pointer; font: 12px system-ui, sans-serif;
    color: ${muted}; border: 1px solid ${rule}; border-radius: 6px; padding: 5px 10px;
    position: fixed; top: 14px; right: 18px; background: ${ground}; z-index: 2147483647;`);
  // The button calls the toggle with the palette it was built with, so leaving
  // reader mode puts the page back exactly as it was.
  const exitPalette = palette;
  exit.addEventListener('click', () => {
    window.__searchReader(exitPalette);
  });
  container.appendChild(exit);
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
  const body = document.createElement('div');
  body.innerHTML = clone.innerHTML;
  container.appendChild(body);

  document.body.innerHTML = '';
  document.body.appendChild(container);
  document.body.scrollTop = 0;
  window.scrollTo(0, 0);
  window.__searchReaderOn = true;
  return true;
};
