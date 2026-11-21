#!/usr/bin/env bash
# Probe P-25, sent by hand after probe.sh, while the labels it made still existed:
# add a label whose name differs only in case from the existing p25:post-a.
# Issue 117 is the issue probe.sh created. Throwaway code.
set -u
R=sapientpants/sf-scratch
printf '{"labels":["P25:POST-A"]}' |
  gh api -i --method POST -H "X-GitHub-Api-Version: 2026-03-10" "repos/$R/issues/117/labels" --input - \
    >responses/Da-post-other-case.txt 2>&1
gh api "repos/$R/labels?per_page=100" \
  --jq '[.[] | select(.name | ascii_downcase | startswith("p25")) | {name, id}]' >responses/Db-labels-after-other-case.json
