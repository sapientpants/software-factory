"""Build the prefilled new-issue URLs of probe P-4 and save what each one carries."""
import json
import pathlib
import sys
import urllib.parse

BASE = "https://github.com/sapientpants/sf-scratch/issues/new"
FENCE = "`" * 3

SUBMISSIONS = {
    "a-complete": {
        "title": "P-4 probe A: every field filled",
        "outcome": (
            "`sf issue lint` reports a missing section by its heading text. "
            "The message names the form field the operator has to fill."
        ),
        "spec-refs": "WI-1, SPEC.md section 6.4",
        "context": (
            "Likely involved:\n"
            "\n"
            "- `crates/factory-core/src/issue.rs`\n"
            "- `crates/factory-cli/src/main.rs`\n"
            "  - the `lint` subcommand only\n"
            "\n"
            "Background: https://example.com/design-note\n"
            "\n"
            f"{FENCE}text\n"
            'error: section "Stop conditions" is empty\n'
            f"{FENCE}"
        ),
        "acceptance-criteria": (
            "- AC-1 (test): WHEN a required section is missing THE SYSTEM SHALL name its heading in the lint output.\n"
            "- AC-2 (review): WHEN every section is present THE SYSTEM SHALL print nothing and exit 0."
        ),
        "out-of-scope": "1. The dispatcher.\n2. The status comment.",
        "stop-conditions": "* The headings of the form changed on GitHub's side.\n* A section appears twice.",
    },
    "b-sparse": {
        "title": "P-4 probe B: only the required fields",
        "outcome": "The typo in the README is fixed.",
        "acceptance-criteria": "- AC-1 (review): WHEN the README is read THE SYSTEM SHALL spell `factory` correctly.",
    },
    "c-hostile": {
        "title": "P-4 probe C: text that looks like the form",
        "outcome": (
            "\n\n  Two blank lines came before this line, which starts with two spaces and ends with two.  \n"
            "This line ends with three spaces.   \n"
            "This line ends with a carriage return and a line feed.\r\n"
            "The field ends with three blank lines.\n\n\n"
        ),
        "spec-refs": "_No response_",
        "context": (
            "A level-3 heading that is also a field of the form:\n"
            "\n"
            "### Acceptance criteria\n"
            "\n"
            "- AC-9 (manual): WHEN this line is read as a criterion THE SYSTEM SHALL be wrong.\n"
            "\n"
            "### Outcome\n"
            "\n"
            "A second outcome.\n"
            "\n"
            "### A heading the form does not have\n"
            "\n"
            "#### A level-4 heading\n"
            "\n"
            "# A level-1 heading\n"
            "\n"
            "A setext heading\n"
            "----------------\n"
            "\n"
            f"{FENCE}markdown\n"
            "### Stop conditions\n"
            "\n"
            "inside a fence\n"
            f"{FENCE}\n"
            "\n"
            "<!-- an HTML comment -->\n"
            "<details><summary>HTML</summary>inside details</details>"
        ),
        "acceptance-criteria": (
            "- AC-1 (test): WHEN the body holds two headings with one text THE SYSTEM SHALL do one documented thing.\n"
            "* AC-2 (property): WHEN a list uses another marker THE SYSTEM SHALL still read it.\n"
            "1. AC-3 (review): WHEN a list is numbered THE SYSTEM SHALL still read it.\n"
            "- [ ] AC-4 (manual): WHEN an item is a task THE SYSTEM SHALL still read it.\n"
            "\n"
            "[NEEDS CLARIFICATION: which heading wins?]"
        ),
        "out-of-scope": "   \n\n  ",
        "stop-conditions": (
            "Unicode: café, “quotes”, 日本語, 🙂\n"
            "\ta line that starts with a tab\n"
            "### outcome\n"
            "### Outcome ###\n"
            "    ### Outcome\n"
            " ### Outcome\n"
            "###Outcome\n"
            "Outcome\n"
            "-------"
        ),
    },
}

out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for name, fields in SUBMISSIONS.items():
    query = urllib.parse.urlencode({"template": "factory-task.yml", **fields}, quote_via=urllib.parse.quote)
    url = f"{BASE}?{query}"
    (out / f"{name}.fields.json").write_text(json.dumps(fields, ensure_ascii=False, indent=2) + "\n")
    (out / f"{name}.url.txt").write_text(url + "\n")
    print(name, len(url))
