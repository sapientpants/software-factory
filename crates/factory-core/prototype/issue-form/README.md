# Probe P-4: how an issue form renders into the issue body

The form, the submissions and the saved issues of probe P-4 of SPEC 23.2 (#14), with a throwaway sketch of the section parser of WI-1. Not for merging. The record is the P-4 section of `docs/research/probes.md`; this directory is the evidence behind it.

| File | What it is |
|---|---|
| `factory-task.yml` | The issue form of SPEC 6.2, as committed to `sapientpants/sf-scratch` at `.github/ISSUE_TEMPLATE/factory-task.yml` (commit `8829957`) |
| `make_urls.py <dir>` | Writes, per submission, the field values and a new-issue URL that prefills the form with them |
| `inputs/*.fields.json` | What each submission put in each field, by field id. A field that is absent was left empty |
| `inputs/*.url.txt` | The URL the operator opened. They pressed Create without changing a field |
| `section_parser.py <inputs> <results>` | The sketch. Compares the sections it reads with the fields that were submitted, and its level-3 headings with the `<h3>` elements of GitHub's HTML |

## Results

The operator created the three issues on 2026-11-20 in the web interface. They were read the same evening with `gh` 2.104.0 on the operator's token.

| Issue on `sf-scratch` | Submission |
|---|---|
| #3 | `a-complete`: every field filled |
| #2 | `b-sparse`: only the two required fields |
| #1 | `c-hostile`: text that looks like the form's own output |

| File | Request |
|---|---|
| `results/issue-<n>.rest.json` | `GET /repos/sapientpants/sf-scratch/issues/<n>` under `X-GitHub-Api-Version: 2026-03-10` |
| `results/issue-<n>.rest-2022-11-28.json` | The same under `2022-11-28` |
| `results/issue-<n>.rest-full.json` | The same under `2026-03-10` with `Accept: application/vnd.github.full+json`, which adds `body_html` and `body_text` |
| `results/issue-<n>.graphql.json` | `repository.issue` through GraphQL: the body and the fields that record edits |
| `results/section-parser.txt` | The output of `section_parser.py inputs results` |

The bodies are whole and as the server sent them; read `body` from the JSON, where a carriage return is still `\r`. No response headers were kept. `GET /issues/<n>/timeline` returned an empty list for each issue and was not saved.

## What the sketch is not

It is a line rule, not a Markdown parser. It knows fenced code blocks, up to three spaces of indentation and the closing run of `#`. It does not know block quotes, list items, HTML blocks or setext headings, so its agreement with GitHub's HTML is shown for these three bodies and no others.
