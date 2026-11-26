#!/usr/bin/env python3
"""fetch.py <url-or-path> <stem> : logged-out fetch of a GitHub diff page, following the lazy loaders.

Saves <stem>.html, then <stem>.frag1.html ... for (a) the compare view's file-list fragment and
(b) each progressive loader (/diffs?...start_entry=...). Writes <stem>.urls with the URLs and UTC times.
"""
import html, re, subprocess, sys, time

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36"
url, stem = sys.argv[1], sys.argv[2]
if url.startswith("/"):
    url = "https://github.com" + url


def get(u, out, xhr):
    cmd = ["curl", "-sL", "-A", UA, "-o", out, "-w", "%{http_code}", u]
    if xhr:
        cmd[1:1] = ["-H", "X-Requested-With: XMLHttpRequest", "-H", "Accept: text/html"]
    for attempt in range(4):
        code = subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
        if code == "200":
            break
        time.sleep(5 * (attempt + 1))
    return code


log = open(stem + ".urls", "w")
cur = stem + ".html"
code = get(url, cur, False)
log.write(f"{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}\t{code}\t{url}\n")
i = 1
while i <= 80:
    h = open(cur, encoding="utf-8", errors="replace").read()
    m = re.search(r'<include-fragment[^>]*\bsrc="([^"]*/compare/file-list\?[^"]*)"', h) or \
        re.search(r'<include-fragment[^>]*\bsrc="([^"]*/diffs\?[^"]*)"[^>]*js-diff-progressive-loader', h)
    if not m:
        break
    u = "https://github.com" + html.unescape(m.group(1))
    cur = f"{stem}.frag{i}.html"
    code = get(u, cur, True)
    log.write(f"{time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())}\t{code}\t{u}\n")
    i += 1
    time.sleep(0.7)
log.close()
print(open(stem + ".urls").read().strip())
