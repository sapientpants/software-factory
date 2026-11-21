#!/usr/bin/env python3
"""Probe P-15: write the text of every case.

Usage: gen.py <results-dir>

Reads <results-dir>/ids.tsv (name, issue number) and writes one file per case
to <results-dir>/payload/. A case is a surface and an encoding. The same seven
encodings are applied to one mention, four forms of an issue reference and one
closing keyword. Once ids.tsv has the targets of the second round, it also
writes that round's text: other spellings of a reference, and two commit
messages for the operator. The same for the third round: two more spellings,
and a commit message. And for the fourth: character references, and a fence
that the text ends.
"""
import pathlib
import sys

OWNER = "sapientpants"
REPO = "sapientpants/sf-scratch"
ZWJ = "\u200d"
ZWSP = "\u200b"

ENCODINGS = ["plain", "fence", "code", "backslash", "zwj", "zwsp", "entity"]
ONE_LINE = [e for e in ENCODINGS if e != "fence"]  # a title or a subject has no room for a fence


def tokens(e, n):
    """The mention and the four reference forms to issue n, in encoding e."""
    at, hash_, gh, full, url = (
        ("@", OWNER),
        ("#", str(n)),
        ("GH-", str(n)),
        (f"{REPO}#", str(n)),
        ("https:", f"//github.com/{REPO}/issues/{n}"),
    )
    pairs = {"mention": at, "hash": hash_, "gh": gh, "full": full, "url": url}
    if e in ("plain", "fence"):
        return {k: a + b for k, (a, b) in pairs.items()}
    if e == "code":
        return {k: f"`{a}{b}`" for k, (a, b) in pairs.items()}
    if e == "zwj":
        return {k: a + ZWJ + b for k, (a, b) in pairs.items()}
    if e == "zwsp":
        return {k: a + ZWSP + b for k, (a, b) in pairs.items()}
    if e == "backslash":  # a backslash before the last character of the first half
        return {k: a[:-1] + "\\" + a[-1] + b for k, (a, b) in pairs.items()}
    if e == "entity":  # that character as a numeric character reference
        return {k: a[:-1] + f"&#{ord(a[-1])};" + b for k, (a, b) in pairs.items()}
    raise ValueError(e)


def lines(e, ids, close):
    t = {form: tokens(e, ids[f"t-{form}"])[form] for form in ("hash", "gh", "full", "url")}
    return [
        f"mention: {tokens(e, 0)['mention']} wrote this.",
        f"hash: see {t['hash']} for the first.",
        f"gh: see {t['gh']} for the second.",
        f"full: see {t['full']} for the third.",
        f"url: see {t['url']} for the fourth.",
        f"close: Fixes {tokens(e, ids[close])['hash']}.",
    ]


def markdown(e, ids, close):
    head = f"Text of probe P-15 in the encoding `{e}`. This line is the host's; the rest stands for agent text."
    body = lines(e, ids, close)
    if e == "fence":
        return head + "\n\n```text\n" + "\n".join(body) + "\n```\n"
    return head + "\n\n" + "\n".join("- " + line for line in body) + "\n"


def commit_body(e, ids, close):
    body = lines(e, ids, close)
    if e == "fence":
        body = ["```text", *body, "```"]
    return f"P-15 probe: commit body, {e}\n\n" + "\n".join(body) + "\n"


def one_line(e, ids, close=None):
    t = tokens(e, 0)["mention"] + " see " + tokens(e, ids["t-hash" if close else "t-title"])["hash"]
    if close:
        t += ", Fixes " + tokens(e, ids[close])["hash"]
    return t


VARIANTS = {  # other spellings of a reference, each with its own target
    "v-noscheme": lambda n: f"github.com/{REPO}/issues/{n}",
    "v-nohost": lambda n: f"{REPO}/issues/{n}",
    "v-zwsp-number": lambda n: f"https://github.com/{REPO}/issues/{ZWSP}{n}",
    "v-zwsp-issues": lambda n: f"https://github.com/{REPO}/{ZWSP}issues/{n}",
    "v-zwsp-owner": lambda n: f"https://github.com/{ZWSP}{REPO}/issues/{n}",
    "v-lower-gh": lambda n: f"gh-{n}",
}


MORE_VARIANTS = {  # the third round; v-pull refers to a pull request of the first round
    "v-zwsp-both": lambda n: f"https:{ZWSP}//github.com/{REPO}/issues/{ZWSP}{n}",
    "v-pull": lambda n: f"https://github.com/{REPO}/pull/{n}",
}


def variants(ids, spellings=VARIANTS):
    return [f"{name}: see {spell(ids['t-' + name])} for it." for name, spell in spellings.items()]


def main():
    out = pathlib.Path(sys.argv[1])
    ids = dict(
        (name, int(n)) for name, n in (l.split("\t") for l in (out / "ids.tsv").read_text().splitlines())
    )
    p = out / "payload"
    p.mkdir(exist_ok=True)
    for e in ENCODINGS:
        (p / f"comment-{e}.md").write_text(markdown(e, ids, "t-close-comment"))
        (p / f"body-{e}.md").write_text(markdown(e, ids, "t-close-body"))
        (p / f"prc-{e}.md").write_text(markdown(e, ids, f"t-close-prc-{e}"))
        (p / f"pre-{e}.md").write_text(markdown(e, ids, f"t-close-pre-{e}"))
        (p / f"cbody-{e}.txt").write_text(commit_body(e, ids, f"t-close-cbody-{e}"))
    for e in ONE_LINE:
        (p / f"title-{e}.txt").write_text(f"P-15 probe: source, title, {e}: " + one_line(e, ids) + "\n")
        (p / f"csubj-{e}.txt").write_text(
            f"P-15 probe: commit subject, {e}: " + one_line(e, ids, f"t-close-csubj-{e}") + "\n"
        )
    if "t-v-nohost" in ids:  # the second round
        (p / "variants.md").write_text(
            "Text of probe P-15: other spellings of a reference.\n\n" + "\n".join("- " + v for v in variants(ids)) + "\n"
        )
        (p / "variants-commit.txt").write_text(
            "P-15 probe: commit body, other spellings\n\n" + "\n".join(variants(ids)) + "\n"
        )
        (p / "operator-empty.txt").write_text(f"P-15 probe: the operator's empty commit, see #{ids['t-op-empty']}\n")
        (p / "operator-file.txt").write_text(f"P-15 probe: the operator's commit of a file, see #{ids['t-op-file']}\n")
    if "t-op-as-bot" in ids:  # the third round
        (p / "variants-more.md").write_text(
            "Text of probe P-15: more spellings of a reference.\n\n"
            + "\n".join("- " + v for v in variants(ids, MORE_VARIANTS)) + "\n"
        )
        (p / "operator-as-bot.txt").write_text(
            f"P-15 probe: a commit in the bot's name that the operator pushed, see #{ids['t-op-as-bot']}\n"
        )
    if "t-c-fence-long" in ids:  # the fourth round
        for name, text in cases(ids).items():
            (p / f"case-{name}.md").write_text(
                f"Text of probe P-15, case `{name}`. This line is the host's; the rest stands for agent text.\n\n{text}\n"
            )
        (p / "variants-charref.md").write_text(
            "Text of probe P-15: a reference spelt with a character reference.\n\n"
            + "\n".join("- " + v for v in variants(ids, CHARREF_VARIANTS)) + "\n"
        )


# The fourth round. A character reference is decoded before GitHub looks for a mention, so
# `&#64;` was a live `@`. Is that so for the other ways to spell one, and does a zero-width
# space after the `&` stop it? The names ending in -z are what such an encoder would write.
Z = ZWSP
CHARREF_VARIANTS = {
    "v-ref-num-raw": lambda n: f"&num;{n}",
    "v-ref-hex-raw": lambda n: f"&#x23;{n}",
    "v-ref-num-z": lambda n: f"&{Z}num;{n}",
    "v-ref-hex-z": lambda n: f"&{Z}#x23;{n}",
    "v-ref-dec-z": lambda n: f"&{Z}#{Z}35;{n}",
    "v-ref-amp": lambda n: f"&amp;#{n};",  # the `&` escaped and nothing else: the text `&#n;`
}


def cases(ids):
    """Issue bodies, one issue each, so that each has its own notification."""
    who = "sapientpants wrote this."
    # Agent text that ends the host's fence and carries on outside it.
    breakout = lambda n: f"A first line.\n```\n\nmention: @{who}\nhash: see #{n} for it.\n"
    return {
        "charref-hex-raw": f"mention: &#x40;{who}",
        "charref-named-raw": f"mention: &commat;{who}",
        "charref-hex-z": f"mention: &{Z}#x40;{who}",
        "charref-named-z": f"mention: &{Z}commat;{who}",
        "charref-dec-z": f"mention: &{Z}#{Z}64;{who}",
        "charref-in-fence": f"```text\nmention: &#64;{who}\nmention: &#x40;{who}\n```",
        "fence-broken": "```text\n" + breakout(ids["t-c-fence-broken"]) + "```",
        "fence-long": "````text\n" + breakout(ids["t-c-fence-long"]) + "````",
    }


if __name__ == "__main__":
    main()
