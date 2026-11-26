#!/bin/sh
# fetch_pr.sh <pr-number> <outdir> [suffix]
# Fetches the logged-out "Files changed" page and follows the progressive loaders.
set -eu
n="$1"; out="$2"; suf="${3:-}"
UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36"
mkdir -p "$out"
main="$out/pr$n-files$suf.html"
curl -sL -A "$UA" -D "$out/pr$n-files$suf.headers" -o "$main" "https://github.com/sapientpants/sf-scratch/pull/$n/files"
date -u +"fetched %Y-%m-%dT%H:%M:%SZ" > "$out/pr$n-files$suf.time"
cur="$main"; i=1
while :; do
  src=$(python3 -I - "$cur" <<'EOF'
import re, sys, html
h = open(sys.argv[1], encoding="utf-8", errors="replace").read()
m = re.search(r'<include-fragment[^>]*\bsrc="([^"]*/diffs\?[^"]*)"[^>]*js-diff-progressive-loader', h)
print(html.unescape(m.group(1)) if m else "")
EOF
)
  [ -z "$src" ] && break
  cur="$out/pr$n-files$suf.frag$i.html"
  echo "$src" > "$out/pr$n-files$suf.frag$i.url"
  curl -sL -A "$UA" -H "X-Requested-With: XMLHttpRequest" -H "Accept: text/html" -o "$cur" "https://github.com$src"
  i=$((i+1))
  [ "$i" -gt 60 ] && break
  sleep 1
done
ls -la "$out"/pr$n-files$suf*.html | awk '{print $5, $9}' | sed "s#$out/##"
