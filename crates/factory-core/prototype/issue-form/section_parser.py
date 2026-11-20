"""Probe P-4: a sketch of the section parser of WI-1, run on real form output.

Throwaway. It reads the issues saved under results/ and the field values that
were submitted (inputs/), and prints, per issue: the headings it finds next to
the ones GitHub rendered, the sections it reads next to the fields that were
typed, and what two simpler parsers would have read.

    python3 section_parser.py inputs results
"""
import html
import json
import pathlib
import re
import sys

# Field id of the form -> heading text (SPEC 6.2), in form order.
FIELDS = {
    "outcome": "Outcome",
    "spec-refs": "Spec refs",
    "context": "Context",
    "acceptance-criteria": "Acceptance criteria",
    "out-of-scope": "Out of scope",
    "stop-conditions": "Stop conditions",
}
NAMES = {name.lower(): name for name in FIELDS.values()}
NO_RESPONSE = "_No response_"

# Which saved issue came from which submission.
ISSUES = {3: "a-complete", 2: "b-sparse", 1: "c-hostile"}

FENCE_OPEN = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
ATX = re.compile(r"^ {0,3}(#{1,6})(?:[ \t]+(.*?))??(?:[ \t]+#+)?[ \t]*$")


def split_lines(body):
    return body.replace("\r\n", "\n").replace("\r", "\n").split("\n")


def atx_headings(lines):
    """(line index, level, text) of each ATX heading outside a fenced code block.

    A line rule, not a Markdown parser: it knows fences, the three spaces of
    indentation and the closing run of '#'. It does not know block quotes,
    list items, HTML blocks or setext headings.
    """
    found = []
    fence = None
    for i, line in enumerate(lines):
        if fence:
            if re.match(r"^ {0,3}" + re.escape(fence[0]) + "{" + str(len(fence)) + r",}[ \t]*$", line):
                fence = None
            continue
        opened = FENCE_OPEN.match(line)
        if opened and not (opened.group(1)[0] == "`" and "`" in opened.group(2)):
            fence = opened.group(1)
            continue
        heading = ATX.match(line)
        if heading:
            found.append((i, len(heading.group(1)), (heading.group(2) or "").strip()))
    return found


def trim_blank_lines(lines):
    lines = list(lines)
    while lines and not lines[0].strip():
        lines.pop(0)
    while lines and not lines[-1].strip():
        lines.pop()
    return "\n".join(lines)


def section_text(lines):
    text = trim_blank_lines(lines)
    return "" if text.strip() == NO_RESPONSE else text


def parse(body):
    """Section name -> the text of every section with that name, in body order."""
    lines = split_lines(body)
    marks = [(i, NAMES[text.lower()]) for i, level, text in atx_headings(lines) if level == 3 and text.lower() in NAMES]
    sections = {}
    for k, (i, name) in enumerate(marks):
        end = marks[k + 1][0] if k + 1 < len(marks) else len(lines)
        sections.setdefault(name, []).append(section_text(lines[i + 1 : end]))
    return sections


def naive(body, keep):
    """Split on lines that start with '### '; keep the first or the last section of a name."""
    sections = {}
    name = None
    for line in split_lines(body):
        if line.startswith("### "):
            name = line[4:]
            if keep == "last" or name not in sections:
                sections[name] = []
                continue
            name = None  # a repeat under first-wins: drop its text
        elif name is not None:
            sections[name].append(line)
    return {name: section_text(lines) for name, lines in sections.items()}


def rendered_h3(body_html):
    return [html.unescape(re.sub(r"<[^>]+>", "", h)) for h in re.findall(r"<h3[^>]*>(.*?)</h3>", body_html, re.S)]


def show(text):
    return json.dumps(text, ensure_ascii=False)


def main(inputs, results):
    ok = True
    for number, submission in ISSUES.items():
        fields = json.loads((inputs / f"{submission}.fields.json").read_text())
        issue = json.loads((results / f"issue-{number}.rest-full.json").read_text())
        body = issue["body"]
        lines = split_lines(body)
        print(f"== issue {number} ({submission}): {len(body.encode())} bytes, {body.count(chr(13))} CR, ends with newline: {body.endswith(chr(10))}")

        mine = [text for _, level, text in atx_headings(lines) if level == 3]
        theirs = rendered_h3(issue["body_html"])
        same = mine == theirs
        ok &= same
        print(f"level-3 headings: sketch {len(mine)}, GitHub's HTML {len(theirs)}, same list: {same}")
        if not same:
            print("  sketch:", mine)
            print("  GitHub:", theirs)

        sections = parse(body)
        repeated = {name: len(texts) for name, texts in sections.items() if len(texts) > 1}
        print(f"repeated section names: {repeated or 'none'}")
        for field, name in FIELDS.items():
            typed = section_text(split_lines(fields.get(field, "")))
            read = sections.get(name, [])
            for keep in ("first", "last"):
                got = naive(body, keep).get(name)
                if got != typed:
                    print(f"  {name}: naive {keep}-wins reads {show(got)}")
            if len(read) == 1 and read[0] == typed:
                print(f"  {name}: round trip holds ({'empty' if not typed else str(len(typed)) + ' characters'})")
            elif len(read) > 1:
                print(f"  {name}: {len(read)} sections, the typed text is number {read.index(typed) + 1 if typed in read else 'none'} of them")
            elif repeated and len(read) == 1 and typed.startswith(read[0]):
                print(f"  {name}: cut short by a section heading typed into it: {len(read[0])} of {len(typed)} characters read")
            else:
                ok = False
                print(f"  {name}: MISMATCH typed {show(typed)} read {show(read)}")
        print()
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])))
