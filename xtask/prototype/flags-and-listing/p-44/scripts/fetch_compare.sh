#!/bin/sh
# fetch_compare.sh <base> <head> <outfile-stem>  (logged-out compare view, follows progressive loaders)
set -eu
base="$1"; head="$2"; stem="$3"
UA="Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36"
curl -sL -A "$UA" -o "$stem.html" "https://github.com/sapientpants/sf-scratch/compare/$base...$head"
cur="$stem.html"; i=1
while :; do
  src=$(python3 -I - "$cur" <<'PY'
import re, sys, html
h = open(sys.argv[1], encoding="utf-8", errors="replace").read()
m = re.search(r'<include-fragment[^>]*\bsrc="([^"]*/diffs\?[^"]*)"[^>]*js-diff-progressive-loader', h)
print(html.unescape(m.group(1)) if m else "")
PY
)
  [ -z "$src" ] && break
  cur="$stem.frag$i.html"
  curl -sL -A "$UA" -H "X-Requested-With: XMLHttpRequest" -H "Accept: text/html" -o "$cur" "https://github.com$src"
  i=$((i+1)); [ "$i" -gt 60 ] && break
  sleep 1
done
