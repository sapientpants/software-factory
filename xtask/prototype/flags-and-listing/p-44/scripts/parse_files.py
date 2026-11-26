#!/usr/bin/env python3
"""parse_files.py <html> [<html> ...] : per-file summary of GitHub classic diff HTML.

Output TSV: path, deleted, state, rows, loader, messages, header-notes
 state: shown (diff table present), collapsed (per-file "Load diff" loader), nocontent (neither)
"""
import html as H
import re
import sys

TAG = re.compile(r"<[^>]+>")


def text(s):
    s = re.sub(r"<svg.*?</svg>", " ", s, flags=re.S)
    s = re.sub(r"<script.*?</script>", " ", s, flags=re.S)
    s = re.sub(r"<template.*?</template>", " ", s, flags=re.S)
    s = TAG.sub(" ", s)
    return re.sub(r"\s+", " ", H.unescape(s)).strip()


rows = []
extra = []
for fn in sys.argv[1:]:
    h = open(fn, encoding="utf-8", errors="replace").read()
    starts = [m.start() for m in re.finditer(r'<div id="diff-[0-9a-f]+"\s+data-details-container-group="file"', h)]
    # page-level messages outside file blocks
    for msg in re.findall(r'(?:Sorry, we could not display the entire diff[^<]*|This diff is taking too long[^<]*|diff is too big[^<]*|truncated[^<]{0,120}|files are hidden[^<]*|not shown because[^<]*|Some content is hidden[^<]*|Large pull request[^<]*|only the first[^<]*)', h):
        extra.append((fn.split("/")[-1], msg.strip()[:300]))
    for k, s in enumerate(starts):
        e = starts[k + 1] if k + 1 < len(starts) else len(h)
        blk = h[s:e]
        head = re.match(r"<div[^>]*>", blk, flags=re.S).group(0)
        path = H.unescape(re.search(r'data-tagsearch-path="([^"]*)"', head).group(1))
        deleted = re.search(r'data-file-deleted="([^"]*)"', head).group(1)
        cls = re.sub(r"\s+", " ", re.search(r'class="([^"]*)"', head, flags=re.S).group(1))
        is_open = "Details--on" in cls
        ci = blk.find("js-file-content")
        header, content = blk[:ci], blk[blk.find(">", ci) + 1:]
        cut = content.find('js-diff-progressive-loader')
        if cut > 0:
            content = content[:content.rfind("<include-fragment", 0, cut)]
        # cut trailing stuff after the file block: keep it simple
        n_rows = len(re.findall(r'<td[^>]*class="[^"]*blob-code[^"]*blob-code-(?:addition|deletion|context)', content))
        n_rows_any = len(re.findall(r'class="[^"]*\bblob-code\b', content))
        has_table = "diff-table" in content
        loader = re.search(r'<include-fragment[^>]*data-fragment-url="([^"]*)"[^>]*js-diff-entry-loader', content)
        hdr_txt = text(header)
        stat = re.search(r"(\d[\d,]* changes?: [\d,]+ additions? & [\d,]+ deletions?|BIN|Binary file[^<]*)", H.unescape(header))
        diffstat = re.search(r'<span class="diffstat[^"]*"[^>]*>(.*?)</span>', header, flags=re.S)
        ds = text(diffstat.group(0)) if diffstat else ""
        # header notes: rename arrows, mode changes, labels
        notes = []
        m = re.search(r'class="Truncate">(.*?)</span>\s*</span>', header, flags=re.S)
        ttl = re.findall(r'<a title="([^"]*)"', header)
        if ttl:
            notes.append("title=" + H.unescape(ttl[0]))
        for lab in re.findall(r'<span[^>]*class="[^"]*Label[^"]*"[^>]*>(.*?)</span>', header, flags=re.S):
            notes.append("label=" + text(lab))
        mm = re.search(r"(\d{6}\s*→\s*\d{6})", H.unescape(text(header)))
        if mm:
            notes.append("mode=" + mm.group(1))
        ctext = text(content)
        if loader:
            state = "collapsed"
        elif has_table and n_rows_any:
            state = "shown"
        else:
            state = "nocontent"
        if has_table and n_rows_any:
            # message = non-code notices inside the content
            msgs = re.findall(r'(No newline at end of file|This file contains (?:hidden or )?bidirectional Unicode text[^.]*\.|File renamed without changes\.?|Whitespace-only changes\.?|Binary file not shown\.?|Empty file\.?|File mode changed\.?)', ctext)
            msg = " | ".join(dict.fromkeys(msgs))
        else:
            msg = ctext[:400]
        rows.append((path, deleted, state, "open" if is_open else "closed", str(n_rows_any), ds, msg, "; ".join(notes)))

print("path\tdeleted\tstate\tdetails\trows\tdiffstat\tmessage\tnotes")
for r in rows:
    print("\t".join(r))
for e in extra:
    print("#PAGE\t" + "\t".join(e))
print(f"#files\t{len(rows)}")
