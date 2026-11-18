#!/usr/bin/env python3
"""A local web server of pages that are hard on a browser.

Every page here exists to reproduce something a person runs into: a shop's
cookie banner, a page that never stops loading, a tab that tries to spray
memory, a form that posts, a download, a redirect loop, forty images, a
document with no reader mode and one with nothing but.

    python3 tools/fixtures.py 8099
"""
import html
import pathlib
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import urlparse, parse_qs

BANNER = """
<div id="hide-me" style="height:60px;background:#f3f3f3">Hide this strip</div>
<div class="cookie-banner" id="cookie-banner">
  <p>We value your privacy and 1,842 partners.</p>
  <button onclick="document.getElementById('cookie-banner').remove()">Accept</button>
</div>
<div class="newsletter-overlay">Subscribe to our newsletter</div>
"""

PAGE = """<!doctype html><html><head><meta charset="utf-8">
<title>{title}</title>
<style>
  body {{ font: 16px/1.5 -apple-system, "Noto Sans", sans-serif; margin: 0; padding: 40px 24px;
         max-width: 740px; margin-inline: auto; color: #222; }}
  h1 {{ font-size: 32px; letter-spacing: -0.01em; }}
  .cookie-banner {{ position: fixed; inset: auto 16px 16px 16px; background: #111; color: #fff;
        padding: 16px; border-radius: 10px; }}
  .newsletter-overlay {{ position: fixed; inset: 0; background: rgba(0,0,0,.55); color: #fff;
        display: grid; place-items: center; font-size: 24px; }}
  img {{ max-width: 100%; }}
  .ad-slot, .adsbygoogle {{ height: 90px; background: #eee; margin: 12px 0; }}
</style></head><body>
{banner}
<h1>{title}</h1>
{body}
</body></html>"""


def page(title, body, banner=True):
    return PAGE.format(title=html.escape(title), body=body, banner=BANNER if banner else "")


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_args):
        pass

    def send(self, body, status=200, kind="text/html; charset=utf-8", headers=None):
        data = body.encode() if isinstance(body, str) else body
        self.send_response(status)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(data)))
        for key, value in (headers or {}).items():
            self.send_header(key, value)
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def do_GET(self):
        path = urlparse(self.path)
        route = path.path
        query = parse_qs(path.query)

        if route == "/office":
            self.send((pathlib.Path(__file__).parent / "demo" / "office.html").read_text())
        elif route == "/titled":
            title = query.get("t", ["Untitled"])[0].replace("+", " ")
            body = (f"<p>Body of {title}.</p>" * 20)
            self.send(page(title, body, banner=False))
        elif route == "/plain":
            self.send(page("Plain page", "<p>The quietest page there is.</p>"))
        elif route == "/long":
            paragraphs = "".join(
                f"<p>Paragraph {i}. " + ("The quick brown fox jumps over the lazy dog. " * 8) + "</p>"
                for i in range(1, 400)
            )
            self.send(page("A long read", f"<article>{paragraphs}</article>"))
        elif route == "/big":
            rows = "".join(f"<div>row {i}</div>" for i in range(8000))
            self.send(page("Eight thousand rows", f"<div>{rows}</div>", banner=False))
        elif route == "/images":
            tiles = "".join(
                f'<img src="/pixel.png?i={i}" width="120" height="90" alt="tile {i}">' for i in range(40)
            )
            self.send(page("Forty images", f"<div>{tiles}</div>", banner=False))
        elif route == "/pixel.png":
            self.send(
                bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010806000000"
                              "1f15c4890000000a49444154789c6360000002000154a24f5f0000000049454e44ae426082"),
                kind="image/png",
            )
        elif route == "/slow":
            delay = min(float(query.get("ms", ["3000"])[0]) / 1000.0, 30.0)
            time.sleep(delay)
            self.send(page("Slow page", f"<p>Served after {delay:.1f}s.</p>"))
        elif route == "/stall":
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Transfer-Encoding", "chunked")
            self.end_headers()
            try:
                self.wfile.write(b"<html><body><h1>Never finishes</h1>" )
                for _ in range(600):
                    self.wfile.write(b"<p>still going</p>")
                    self.wfile.flush()
                    time.sleep(0.1)
            except (BrokenPipeError, ConnectionResetError):
                pass
        elif route == "/redirect":
            hops = int(query.get("n", ["1"])[0])
            target = f"/redirect?n={hops - 1}" if hops > 1 else "/plain"
            self.send_response(302)
            self.send_header("Location", target)
            self.send_header("Content-Length", "0")
            self.end_headers()
        elif route == "/loop":
            self.send_response(302)
            self.send_header("Location", "/loop")
            self.send_header("Content-Length", "0")
            self.end_headers()
        elif route == "/missing":
            self.send("not found", status=404, kind="text/plain")
        elif route == "/banner":
            # A page whose whole content is the kind of thing the blocker hides,
            # next to one thing it must leave alone. The two are told apart by
            # measuring them in the page, not by trusting the rule list.
            self.send(page("Banner", "<p id='keep'>This stays.</p>"
                           "<div class='cookie-banner'>A banner</div>"
                           "<div class='ad-slot'>An advertisement</div>"
                           "<div class='newsletter-overlay'>Newsletter</div>"
                           "<script>window.__probe=()=>{"
                           "const seen=(sel)=>{const n=document.querySelector(sel);"
                           "return !!(n&&n.getBoundingClientRect().width>0&&getComputedStyle(n).display!=='none')};"
                           "return JSON.stringify({keep:seen('#keep'),banner:seen('.cookie-banner'),"
                           "ad:seen('.ad-slot'),news:seen('.newsletter-overlay')})}</script>",
                           banner=False))
        elif route == "/cookie":
            # Two routes, because one route cannot answer both questions: "/cookie?set=1"
            # puts a cookie in the jar, "/cookie" only reports what is in it. A single
            # route that both sets and reads would let a private tab plant the very
            # cookie the test was checking it did not have.
            # The value can be named, so two jars can be told apart: a tab that
            # writes "flavour=private" must not be visible from another one.
            value = query.get("set", [""])[0] or "oat"
            headers = {"Set-Cookie": f"flavour={value}; Path=/"} if "set" in query else None
            self.send(page("Cookie jar", "<p id='who'>nobody</p>"
                           "<script>document.getElementById('who').textContent=document.cookie||'empty'</script>",
                           banner=False), headers=headers)
        elif route == "/js-busy":
            self.send(page("Busy script", "<p>Working...</p><script>const t=Date.now();"
                                           "while(Date.now()-t<20000){}</script>", banner=False))
        elif route == "/js-leak":
            self.send(page("Memory spray", "<p>Allocating.</p><script>"
                                           "window.holes=[];setInterval(()=>{for(let i=0;i<2000;i++){"
                                           "holes.push(new Array(1000).fill(Math.random()))}},50)</script>",
                           banner=False))
        elif route == "/frames":
            frames = "".join(f'<iframe src="/plain?f={i}" width="300" height="180"></iframe>' for i in range(12))
            self.send(page("Twelve frames", frames, banner=False))
        elif route == "/popup":
            self.send(page("Popup", "<p><button id='go' onclick=\"window.open('/plain','_blank')\">"
                                    "Open one</button></p>", banner=False))
        elif route == "/download":
            payload = b"search-download-fixture\n" * 4096
            self.send(payload, kind="application/octet-stream",
                      headers={"Content-Disposition": 'attachment; filename="fixture.bin"'})
        elif route == "/form":
            self.send(page("A form", "<form action='/plain' method='get'>"
                                     "<input name='q' value='hello'><button id='send'>Send</button></form>",
                           banner=False))
        elif route == "/fonts":
            self.send(page("Type", "<h1>Handgloves 0123</h1><p style='font-size:19px'>"
                                   "The five boxing wizards jump quickly. 0123456789</p>", banner=False))
        else:
            self.send(page("Fixture index",
                           "<ul>" + "".join(
                               f"<li><a href='{name}'>{name}</a></li>" for name in
                               ["plain", "long", "big", "images", "slow", "stall", "redirect?n=3",
                                "loop", "missing", "js-busy", "js-leak", "frames", "popup",
                                "download", "form", "fonts", "banner", "cookie"]
                           ) + "</ul>"))


def main(argv):
    port = int(argv[1]) if len(argv) > 1 else 8099
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.daemon_threads = True
    print(f"fixtures on http://127.0.0.1:{port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main(sys.argv)
