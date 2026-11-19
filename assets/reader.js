// Reading mode, done in the page: take the article out of the page it is in
// and set it again in the browser's own type. No second view, no extra
// process, and Back still works because the address never changes.
window.__searchReader = function (palette) {
  if (window.__searchReaderOn) {
    window.__searchReaderOn = false;
    const styles = window.__searchReaderStyles || [null, null];
    const y = Number(window.__searchReaderScroll) || 0;
    // The page's own markup, put back from the string the enter path kept.
    // Written here, not eval'ed from a stored expression: the exit path is a
    // constant, so nothing in the page can change what it runs, and a page
    // whose policy blocks eval still gets its words back.
    const source = window.__searchReaderBody;
    // Without the page we saved there is nothing to go back to. Saying `left`
    // here would tell the host the tab is out of reading mode while the
    // article is still on screen.
    if (typeof source !== 'string') {
      window.__searchReaderOn = true;
      return 'failed';
    }
    try {
      document.body.innerHTML = source;
      window.__searchReaderBody = null;
    } catch (_) {
      // Half a page is worse than the reader it replaces. Stay in and say so,
      // so the tab's flag keeps matching the page it belongs to, and keep the
      // source so a later try can still put the page back.
      window.__searchReaderOn = true;
      return 'failed';
    }
    document.head.querySelectorAll('style[data-search-reader]').forEach((n) => n.remove());
    // Put back the exact attribute values the page had, null meaning "no attribute".
    [[document.body, styles[0]], [document.documentElement, styles[1]]].forEach(([node, value]) => {
      if (value === null) node.removeAttribute('style');
      else node.setAttribute('style', value);
    });
    const pageY = Number(window.__searchReaderScroll) || 0;
    window.scrollTo(0, pageY);
    window.__searchReaderStyles = null;
    window.__searchReaderScroll = null;
    // Retry after late layout, not before the immediate restore has run.
    requestAnimationFrame(() => window.scrollTo(0, pageY));
    window.addEventListener('load', () => window.scrollTo(0, pageY), { once: true });
    return 'left';
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
  if (!best) return 'empty';

  const clone = best.cloneNode(true);
  const junk = 'script, style, nav, aside, form, iframe, noscript, footer, header, button, [class*=share], [class*=comment], [class*=newsletter], [class*=cookie], [class*=sidebar], [class*=related], [aria-hidden=true]';
  clone.querySelectorAll(junk).forEach((n) => n.remove());

  const heading =
    (document.querySelector('h1') || {}).innerText ||
    document.title ||
    '';
  const bylineNode = document.querySelector('[rel=author], .byline, .author, [class*=byline]');
  const byline = bylineNode ? (bylineNode.innerText || '').trim().slice(0, 140) : '';

  window.__searchReaderScroll = window.scrollY;
  window.__searchReaderStyles = [
    document.body.getAttribute('style'),
    document.documentElement.getAttribute('style'),
  ];
  // The exit path restores from this string directly; nothing here is code.
  window.__searchReaderBody = document.body.innerHTML;

  // Palette owns the pairs; keeping a second set here loses their contrast.
  const tokens = palette;
  const pageCss = tokens.palette;
  const rule = tokens.rule;
  const muted = tokens.muted;
  const ground = tokens.ground;

  document.head.querySelectorAll('[data-search-reader]').forEach((n) => n.remove());
  const style = document.createElement('style');
  style.setAttribute('data-search-reader', '');
  style.textContent = `
    html, body { ${pageCss} !important; }
    body { margin: 0 auto !important; max-width: 720px !important; font: 19px/1.62 "SF Pro Text", Inter, "Adwaita Sans", "Noto Sans", sans-serif !important;
      -webkit-font-smoothing: antialiased; }
    #__search_reader { box-sizing: border-box; max-width: 660px; margin: 0 auto; padding: 72px 28px 120px; }
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
  // Fixed at the top right of the page column. The find bar in src/find.rs
  // places itself at the same inset and top (PAGE_INSET, PAGE_TOP): the two
  // are one pair, and moving one alone leaves two different edges on screen.
  exit.setAttribute('style', `all: unset; display: inline-block; cursor: pointer; font: 12px system-ui, sans-serif;
    color: ${muted}; border: 1px solid ${rule}; border-radius: 6px; padding: 5px 10px;
    position: fixed; top: 14px; right: 18px; background: ${ground}; z-index: 2147483647;`);
  // The button calls the toggle with the tokens it was built with, so it can
  // enter again with real colours if a restore ever leaves it on screen.
  const exitPalette = tokens;
  exit.addEventListener('click', () => {
    const answer = window.__searchReader(exitPalette);
    // Tell the host too; it only knows about its own toggle. Only a real exit
    // is reported: a failed restore is not a tab that left reading mode.
    // Guarded: the handler is absent under any other WebView.
    if (answer !== 'left') return;
    try {
      window.webkit.messageHandlers.searchReader.postMessage('off');
    } catch (_) {}
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
  return 'entered';
};
