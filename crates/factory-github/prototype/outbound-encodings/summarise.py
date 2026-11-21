#!/usr/bin/env python3
"""Probe P-15: one line per case from an observation.

Usage: summarise.py <results-dir> <observation-name>

Writes <results-dir>/<observation-name>/summary.tsv and prints it. A case is a
surface and an encoding. Once the second round has run it also writes
summary-second.tsv. The columns say what GitHub did with the text:

  mention   link   the rendered HTML has a link of class user-mention
            event  the source has a `mentioned` event
            note   the reason of the operator's notification for the source
  hash, gh, full, url
            link   the rendered HTML links to the target issue
            xref   the target has a cross-reference from the source,
                   or, for a commit, a `referenced` event that names it
  close     xref   as above, for the target of the closing keyword
            will   that cross-reference has willCloseTarget, or the pull
                   request lists the target among those it closes
            state  the target's state, and what closed it
"""
import json
import pathlib
import re
import sys

ENCODINGS = ["plain", "fence", "code", "backslash", "zwj", "zwsp", "entity"]
SURFACES = ["comment", "body", "title", "prc", "pre", "csubj", "cbody", "cedit", "bedit", "csubj-op", "cbody-op"]
TITLES = {"comment": "source, comment, {e}", "body": "source, body, {e}", "title": "source, title, {e}:",
          "cedit": "source, comment edited, {e}", "bedit": "source, body edited, {e}"}
REPO_URL = "https://api.github.com/repos/sapientpants/sf-scratch"


def main():
    out = pathlib.Path(sys.argv[1])
    obs = out / sys.argv[2]
    ids = dict((k, int(v)) for k, v in (l.split("\t") for l in (out / "ids.tsv").read_text().splitlines()))
    items = {i["number"]: i for i in json.loads((obs / "items.json").read_text())}
    commits = json.loads((obs / "commits.json").read_text())  # the workflow's
    bots = {c["sha"] for c in commits}
    rendered = json.loads((obs / "commits-html.json").read_text()) if (obs / "commits-html.json").exists() else []
    commit_html = {c["oid"]: (c["messageHeadlineHTML"] or "") + "\n" + (c["messageBodyHTML"] or "") for c in rendered}
    notes = {n["subject"]["url"]: n["reason"] for n in json.loads((obs / "notifications.json").read_text())}

    def events(n, kind):
        return [ev for ev in items[n]["timelineItems"]["nodes"] if ev.get("__typename") == kind]

    def xref(target, source):
        """The reference from a source (a number or a commit) recorded on a target, or None."""
        if isinstance(source, int):
            return next((ev for ev in events(target, "CrossReferencedEvent") if ev["source"].get("number") == source), None)
        return next((ev for ev in events(target, "ReferencedEvent") if ev["commit"] and ev["commit"]["oid"] == source), None)

    def linked(html, target):
        return bool(html) and re.search(rf'href="https://github\.com/sapientpants/sf-scratch/(issues|pull)/{target}"', html) is not None

    def yn(v):
        return "yes" if v else "no"

    rows = []
    for s in SURFACES:
        for e in ENCODINGS:
            base = s[:-3] if s.endswith("-op") else s  # -op: the operator's commit of the same message
            if e == "fence" and base in ("title", "csubj"):
                continue
            html = None
            if s in TITLES:
                want = "P-15 probe: " + TITLES[s].format(e=e)
                src = next((n for n, i in items.items() if i["title"].startswith(want)), None)
                if src is None:  # the second round: not in the first three observations, and not every encoding
                    continue
                it = items[src]
                html = {"comment": lambda: it["comments"]["nodes"][0]["bodyHTML"], "body": lambda: it["bodyHTML"],
                        "title": lambda: it["titleHTML"]}[{"cedit": "comment", "bedit": "body"}.get(s, s)]()
                url, name = f"{REPO_URL}/issues/{src}", f"#{src}"
            elif s in ("prc", "pre"):
                src = ids[f"pr-{s}-{e}"]
                it = items[src]
                html = it["bodyHTML"]
                url, name = f"{REPO_URL}/pulls/{src}", f"#{src}"
            else:
                subject = f"P-15 probe: commit {'subject' if base == 'csubj' else 'body'}, {e}"
                if s == base:
                    src = next(c["sha"] for c in commits if c["message"].split("\n")[0].startswith(subject))
                else:
                    src = next((c["oid"] for c in rendered
                                if c["messageHeadline"].startswith(subject) and c["oid"] not in bots), None)
                    if src is None:  # the third round
                        continue
                it = None
                html = commit_html.get(src)
                url, name = f"{REPO_URL}/commits/{src}", src[:7]

            row = {"surface": s, "encoding": e, "source": name}
            row["mention.link"] = yn("user-mention" in html) if html is not None else "-"
            row["mention.event"] = yn(events(src, "MentionedEvent")) if it else "-"
            row["mention.note"] = "-" if s != base else notes.get(url, "none")
            forms = ["hash"] if base in ("title", "csubj") else ["hash", "gh", "full", "url"]
            for f in ["hash", "gh", "full", "url"]:
                if f not in forms:
                    row[f"{f}.link"] = row[f"{f}.xref"] = "-"
                    continue
                t = ids["t-title"] if s == "title" else ids[f"t-{f}"]
                row[f"{f}.link"] = yn(linked(html, t)) if html is not None else "-"
                row[f"{f}.xref"] = yn(xref(t, src))
            if s == "title":
                row["close.xref"] = row["close.will"] = row["close.state"] = "-"
            else:
                shared = {"comment": "t-close-comment", "body": "t-close-body", "cedit": "t-close-comment", "bedit": "t-close-body"}
                t = ids[shared.get(s, f"t-close-{base}-{e}")]
                x = xref(t, src)
                row["close.xref"] = yn(x)
                will = bool(x and x.get("willCloseTarget"))
                if it is None:  # a commit: the rendered message marks the keyword
                    will = bool(html) and re.search(rf'issue-keyword[^>]*>\w+</span> <a [^>]*issues/{t}"', html) is not None
                if s in ("prc", "pre"):
                    will = will or t in [n["number"] for n in it["closingIssuesReferences"]["nodes"]]
                row["close.will"] = yn(will)
                state = items[t]["state"].lower()
                for ev in events(t, "ClosedEvent"):
                    c = ev.get("closer") or {}
                    state += " by " + (f"#{c['number']}" if "number" in c else c.get("oid", "?")[:7] if c else "no closer")
                row["close.state"] = state
            rows.append(row)

    table(obs / "summary.tsv", rows)

    # The second and third rounds: other spellings of a reference, in a comment and in a commit
    # message, and the commits that only ask who leaves a `referenced` event.
    #   comment.link, comment.xref   as above, for the comment that holds the spelling
    #   commit.link                  the rendered message of the workflow's commit links to the target
    #   event.bot, event.operator    the target has a `referenced` event that names the workflow's
    #                                commit, or the operator's commit of the same message
    sources = [n for n, i in items.items() if i["title"].endswith("spellings")]
    if not sources:
        return
    by_subject = {}
    for c in rendered:
        by_subject.setdefault(c["messageHeadline"], []).append(c["oid"])
    branch = {c["oid"]: c.get("branch") for c in rendered}
    spelt = by_subject.get("P-15 probe: commit body, other spellings", [])
    # A read made before observe.sh recorded branches has the workflow's commit only.
    bot = next((o for o in spelt if branch[o] == "probe/p-15-variants"), spelt[0])
    rows = []
    for name in [k for k in ids if k.startswith("t-v-")]:
        t = ids[name]
        src = next((n for n in sources if name[2:] + ":" in items[n]["comments"]["nodes"][0]["body"]), None)
        if src is None:  # a spelling of the third round, in a read made before it
            continue
        in_commit = name in ("t-v-noscheme", "t-v-nohost", "t-v-zwsp-number", "t-v-zwsp-issues", "t-v-zwsp-owner", "t-v-lower-gh")
        rows.append({"case": name[2:], "target": f"#{t}",
                     "comment.link": yn(linked(items[src]["comments"]["nodes"][0]["bodyHTML"], t)),
                     "comment.xref": yn(xref(t, src)),
                     "commit.link": yn(linked(commit_html[bot], t)) if in_commit else "-",
                     "event.bot": yn(xref(t, bot)) if in_commit else "-",
                     "event.operator": yn(any(xref(t, o) for o in spelt if o != bot)) if in_commit and len(spelt) > 1 else "-"})
    for name in ("t-op-empty", "t-op-file", "t-op-as-bot"):
        if name not in ids:
            continue
        t = ids[name]
        oid = next((o for s_, os_ in by_subject.items() if s_.endswith(f"see #{t}") for o in os_), None)
        if oid is None:
            continue
        rows.append({"case": name[2:], "target": f"#{t}", "comment.link": "-", "comment.xref": "-",
                     "commit.link": yn(linked(commit_html[oid], t)), "event.bot": "-", "event.operator": yn(xref(t, oid))})
    print()
    table(obs / "summary-second.tsv", rows)

    # The fourth round: one issue for each case, with the text as its body.
    #   ref.link, ref.xref   for the two cases that refer to an issue
    rows = []
    for n, it in sorted(items.items()):
        if not it["title"].startswith("P-15 probe: source, body, case "):
            continue
        name = it["title"].rsplit(" ", 1)[1]
        t = ids.get(f"t-c-{name}")
        rows.append({"case": name, "source": f"#{n}", "mention.link": yn("user-mention" in it["bodyHTML"]),
                     "mention.note": notes.get(f"{REPO_URL}/issues/{n}", "none"),
                     "ref.link": yn(linked(it["bodyHTML"], t)) if t else "-", "ref.xref": yn(xref(t, n)) if t else "-"})
    if rows:
        print()
        table(obs / "summary-fourth.tsv", rows)


def table(path, rows):
    cols = list(rows[0])
    text = "\n".join(["\t".join(cols)] + ["\t".join(r[c] for c in cols) for r in rows]) + "\n"
    path.write_text(text)
    widths = [max(len(c), *(len(r[c]) for r in rows)) for c in cols]
    for line in text.splitlines():
        print("  ".join(v.ljust(w) for v, w in zip(line.split("\t"), widths)).rstrip())


if __name__ == "__main__":
    main()
