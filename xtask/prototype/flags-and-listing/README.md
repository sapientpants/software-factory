# Probes P-43 and P-44: throwaway scripts and their output

For issue #94. The records are in `docs/research/probes.md` on `main`. Nothing here is built or tested, and this branch is never merged.

- `p-43/`: which configuration files the check tools read unasked. `scripts/` holds one script for each tool and the source of the interposer that logged file lookups; `results/` holds the output of each script, of one run on 2026-11-26.
- `p-44/`: what GitHub's page of a pull request shows no content of. `scripts/` generates the cases, pushes them to `sapientpants/sf-scratch`, fetches the pages and parses them; `results/` holds the manifests of the planted files, the parse of each page and the table of the size experiments. The pages themselves, 85 MB of HTML, are not kept.

Paths of the machine are replaced: `<scratch>` is the directory the scripts ran in, `<home>` the home directory.
