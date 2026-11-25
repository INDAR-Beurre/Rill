// Reading mode, done in the page: take the article out of the page it is in
// and set it again in the browser's own type. No second view, no extra
// process, and Back still works because the address never changes.
// The reader owns its type size for this session. The host can drive these
// buttons through the bench `js` verb, and a person can click A- / A+.
window.__searchReaderFont = function (delta) {
  const current = Number(window.__searchReaderSize) || 18;
  const next = Math.max(14, Math.min(22, current + delta));
  window.__searchReaderSize = next;
  document.documentElement.style.setProperty('--search-reader-size', next + 'px');
  return String(next);
};

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
  const junk = 'script, style, nav, aside, form, iframe, noscript, footer, header, button, svg, object, embed, [class*=share], [class*=comment], [class*=newsletter], [class*=cookie], [class*=sidebar], [class*=related], [aria-hidden=true]';
  clone.querySelectorAll(junk).forEach((n) => n.remove());
  const isUnsafeUrl = (val) => {
    const clean = String(val || '').replace(/[\u0000-\u0020\u007f]+/g, '').toLowerCase();
    return clean.startsWith('javascript:') || clean.startsWith('vbscript:');
  };
  for (const el of [clone, ...clone.querySelectorAll('*')]) {
    if (!el.attributes) continue;
    for (const attr of Array.from(el.attributes)) {
      const name = attr.name.toLowerCase();
      if (name.startsWith('on')) {
        el.removeAttribute(attr.name);
      } else if (
        (name === 'href' || name === 'src' || name === 'xlink:href') &&
        isUnsafeUrl(attr.value)
      ) {
        el.removeAttribute(attr.name);
      }
    }
  }

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
    body { margin: 0 auto !important; padding: 0 !important; max-width: 720px !important; font: var(--search-reader-size, 18px)/1.62 Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif !important;
      -webkit-font-smoothing: antialiased; }
    #__search_reader { box-sizing: border-box; max-width: 660px; margin: 0 auto; padding: 48px 28px 120px; }
    #__search_reader h1 { font-size: 1.79em; line-height: 1.18; letter-spacing: -0.012em; font-weight: 700; margin: 0 0 12px; }
    /* Secondary text is sized in em from the 18px body instead of fixed
       pixels, so it follows the reader's text size and page zoom along with
       everything else. 0.76em of 18px is about 13.7px. */
    #__search_reader .byline { color: ${muted}; font-size: 0.76em; margin: 0 0 40px; }
    #__search_reader p, #__search_reader li, #__search_reader blockquote { margin: 0 0 1.05em; }
    #__search_reader h2, #__search_reader h3 { line-height: 1.25; margin: 1.8em 0 0.5em; letter-spacing: -0.01em; }
    #__search_reader h2 { font-size: 1.32em; font-weight: 600; }
    #__search_reader h3 { font-size: 1.11em; font-weight: 600; letter-spacing: -0.005em; }
    #__search_reader h4, #__search_reader h5, #__search_reader h6 { font-size: 1em; font-weight: 600; line-height: 1.3; margin: 1.6em 0 0.4em; }
    #__search_reader img, #__search_reader video, #__search_reader figure { max-width: 100%; height: auto; border-radius: 6px; }
    /* A long bare URL has no spaces to break at, so without this it would push
       past the 604px measure. Break anywhere only when the line would overflow. */
    #__search_reader a { color: inherit; text-decoration: underline; text-underline-offset: 3px; text-decoration-color: ${rule}; overflow-wrap: anywhere; }
    #__search_reader blockquote { border-left: 2px solid ${rule}; padding-left: 18px; color: ${muted}; }
    /* Inline code and code blocks are styled apart. Inline code sits in running
       prose, so it is sized from the text around it (0.86em, about 16px in 19px
       body text, and it scales inside headings too) and gets a little padding
       so the tint does not touch the letters. Blocks keep the smaller 0.76em,
       their own line height and the 14px inset. */
    #__search_reader code { font-family: "SF Mono", "DejaVu Sans Mono", monospace; font-size: 0.86em; background: ${rule}; border-radius: 4px; padding: 0.12em 0.36em; -webkit-box-decoration-break: clone; box-decoration-break: clone; overflow-wrap: anywhere; }
    /* Code blocks wrap instead of scrolling sideways. Reading mode is for
       reading, and a horizontal scrollbar inside a 604px measure hides the
       end of each long line (Gemini R59 fix 4). pre-wrap keeps the block's
       own line breaks and indentation and only wraps lines that would
       overflow. overflow-wrap: anywhere breaks a long token with no spaces,
       such as a URL or a hash. overflow-x: auto stays as a fallback for
       anything that still cannot break. */
    #__search_reader pre { font: 0.76em/1.5 "SF Mono", "DejaVu Sans Mono", monospace; background: ${rule}; border-radius: 6px; padding: 14px; white-space: pre-wrap; overflow-wrap: anywhere; overflow-x: auto; }
    /* Code inside a block takes the block's size, not 0.86 of 0.76, and has no tint or padding of its own.
       It inherits the block's wrapping, so a long line inside <pre><code> wraps too. */
    #__search_reader pre code { font-size: 1em; background: none; padding: 0; border-radius: 0; white-space: inherit; overflow-wrap: inherit; }
    #__search_reader hr { border: none; border-top: 1px solid ${rule}; margin: 2.4em 0; }
    #__search_reader .search-reader-origin { font-size: 0.68em; color: ${muted}; margin-bottom: 28px; }
    /* The exit button's inline style starts with all: unset, which removes the
       browser's default focus ring, and nothing replaced it. A stylesheet rule
       marked !important beats an inline declaration without !important, so
       these rules win over that reset. Hover makes the border stronger. Keyboard
       focus gets a visible 2px ring set 2px off the edge, so the way out can be
       found without a mouse. */
    #__search_reader_exit:hover { border-color: ${muted} !important; }
    #__search_reader_exit:focus-visible { outline: 2px solid ${muted} !important; outline-offset: 2px !important; border-color: ${muted} !important; }
    /* The type controls sit on the opposite side from the exit pill and the
       find bar, so all three can be captured and used at once. */
    #__search_reader_font { position: fixed; top: 14px; left: 18px; z-index: 2147483647; display: flex; gap: 6px; }
    #__search_reader_font button { all: unset; box-sizing: border-box; min-width: 32px; min-height: 28px; cursor: pointer; text-align: center;
      font: 13px/18px Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif;
      color: ${muted}; border: 1px solid ${rule}; border-radius: 6px; background: ${ground}; }
    #__search_reader_font button:hover { border-color: ${muted} !important; }
    #__search_reader_font button:focus-visible { outline: 2px solid ${muted} !important; outline-offset: 2px !important; }
    /* Find selects the current match, and WebKit paints that selection in its
       own bright yellow while keeping the page's text colour. On the dark
       ground that put light text on yellow, which is hard to read. The
       selection now uses dark text on a warm amber in both looks, so the
       current match reads the same way in light and dark. Ordinary text
       selection uses the same colours. */
    #__search_reader ::selection, html::selection, body::selection { background: #f2c94c !important; color: #1b1a17 !important; }
  `;
  document.head.appendChild(style);
  window.__searchReaderFont(0);

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
  // Same font as the browser chrome, and a 28px minimum height, so the way
  // out looks like a browser control and not like part of the page.
  exit.setAttribute('style', `all: unset; display: inline-block; box-sizing: border-box; min-height: 28px; cursor: pointer;
    font: 13px/18px Inter, "Adwaita Sans", "Noto Sans", Cantarell, sans-serif;
    color: ${muted}; border: 1px solid ${rule}; border-radius: 6px; padding: 4px 10px;
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
  const font = document.createElement('div');
  font.id = '__search_reader_font';
  for (const [label, delta, name] of [['A-', -4, 'Smaller reading text'], ['A+', 4, 'Larger reading text']]) {
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = label;
    button.title = name;
    button.setAttribute('aria-label', name);
    const step = delta;
    button.addEventListener('click', () => window.__searchReaderFont(step));
    font.appendChild(button);
  }
  container.appendChild(font);
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
