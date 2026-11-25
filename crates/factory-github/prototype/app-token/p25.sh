#!/usr/bin/env bash
# Probe P-25 again: adding a label that does not exist, under the scratch App's
# installation token, which holds the Issues permission and no admin permission.
# Then the label calls that GH-3a and step 5 of 20.1 make. Throwaway code.
#
# usage: p25.sh <output directory>
set -u
cd "$(dirname "$0")" && . ./lib.sh
OUT=$1
mkdir -p "$OUT"
mint_token
op_token

labels() { jq -cn '{labels: $ARGS.positional}' --args "$@"; }
# label_read <name> <label>: does the label exist, read with the App's token.
label_read() { call "$1" app "$V1" GET "repos/$R/labels/$2"; }

# What the App may do on the repository, as GitHub states it to each token.
call 00a-repository-app app "$V1" GET "repos/$R"
jq -c '{permissions}' <<<"$BODY"

# The read of GH-3a.
call 00b-list-labels app "$V1" GET "repos/$R/labels?per_page=100"
jq -c '[.[].name]' <<<"$BODY"

# The issue every label call is made on, filed by the App.
call 01-create-issue app "$V1" POST "repos/$R/issues" \
  '{"title":"P-25 probe: adding a label that does not exist, as the scratch App","body":"Probe P-25 of the software factory specification, run again under the scratch App'"'"'s installation token (issue 82 of the factory repository). Labels named p25app:* are created and deleted by the probe."}'
N=$(jq -r .number <<<"$BODY")
echo "$N" >"$OUT/issue-number.txt"
echo "issue: $N"

for pair in "a $V1" "b $V2"; do
  set -- $pair
  t=$1
  v=$2

  # POST /issues/{n}/labels: the add-label endpoint of GH-1.
  label_read "1$t-post-before" "p25app:post-$t"
  call "2$t-post" app "$v" POST "repos/$R/issues/$N/labels" "$(labels "p25app:post-$t")"
  label_read "3$t-post-after" "p25app:post-$t"

  # PUT /issues/{n}/labels: replaces the whole set, which GH-1 forbids.
  label_read "4$t-put-before" "p25app:put-$t"
  call "5$t-put" app "$v" PUT "repos/$R/issues/$N/labels" "$(labels "p25app:put-$t")"
  label_read "6$t-put-after" "p25app:put-$t"

  # PATCH /issues/{n} with a labels key.
  label_read "7$t-patch-before" "p25app:patch-$t"
  call "8$t-patch" app "$v" PATCH "repos/$R/issues/$N" "$(labels "p25app:patch-$t")"
  label_read "9$t-patch-after" "p25app:patch-$t"

  # POST /issues with a labels key: a new issue born with the label.
  label_read "A$t-create-before" "p25app:create-$t"
  call "B$t-create" app "$v" POST "repos/$R/issues" \
    "$(jq -cn --arg t "P-25 probe: created by the scratch App with a label that does not exist ($v)" --arg l "p25app:create-$t" '{title: $t, labels: [$l]}')"
  echo "$(jq -r .number <<<"$BODY")" >>"$OUT/issue-number.txt"
  label_read "C$t-create-after" "p25app:create-$t"
done

# A name in another letter case than an existing label, on each of the four calls.
call D1-post-other-case app "$V1" POST "repos/$R/issues/$N/labels" "$(labels "P25APP:POST-A")"
jq -c '[.[] | {id, name}]' <<<"$BODY"
call D2-put-other-case app "$V1" PUT "repos/$R/issues/$N/labels" "$(labels "P25APP:PUT-A")"
jq -c '[.[] | {id, name}]' <<<"$BODY"
call D3-patch-other-case app "$V1" PATCH "repos/$R/issues/$N" "$(labels "P25APP:PATCH-A")"
jq -c '[.labels[] | {id, name}]' <<<"$BODY"
call D4-create-other-case app "$V1" POST "repos/$R/issues" \
  '{"title":"P-25 probe: created by the scratch App with a label in another letter case","labels":["P25APP:CREATE-A"]}'
jq -c '[.labels[] | {id, name}]' <<<"$BODY"
echo "$(jq -r .number <<<"$BODY")" >>"$OUT/issue-number.txt"
label_read D5-read-other-case "P25APP:POST-A"
jq -c '{id, name}' <<<"$BODY"

# The calls of step 5 of 20.1 and of GH-1, with the App's token: create a label with
# a colour and a description, change it, remove a label from an issue, delete a label.
call E1-create-label app "$V1" POST "repos/$R/labels" \
  '{"name":"p25app:explicit","color":"0e8a16","description":"Created by the scratch App through POST /labels"}'
call E2-create-label-again app "$V1" POST "repos/$R/labels" '{"name":"p25app:explicit","color":"0e8a16"}'
call E3-create-label-other-case app "$V1" POST "repos/$R/labels" '{"name":"P25APP:EXPLICIT","color":"0e8a16"}'
call E4-update-label app "$V1" PATCH "repos/$R/labels/p25app:explicit" '{"description":"Changed by the scratch App"}'
call E5-add-explicit app "$V1" POST "repos/$R/issues/$N/labels" "$(labels "p25app:explicit")"
call E6-remove-label app "$V1" DELETE "repos/$R/issues/$N/labels/p25app:explicit"
call E7-remove-label-absent app "$V1" DELETE "repos/$R/issues/$N/labels/p25app:explicit"
call E8-remove-label-no-such app "$V1" DELETE "repos/$R/issues/$N/labels/p25app:no-such-label"

# What the issue and its timeline say afterwards.
call Z1-issue-labels app "$V1" GET "repos/$R/issues/$N/labels"
call Z2-timeline app "$V1" GET "repos/$R/issues/$N/timeline?per_page=100"
jq -c '.[] | {event, actor: .actor.login, label: .label.name, created_at}' <<<"$BODY"
call Z3-repo-labels app "$V1" GET "repos/$R/labels?per_page=100"
jq -c '[.[] | select(.name | ascii_downcase | startswith("p25app")) | {name, color, description}]' <<<"$BODY"

# Cleanup, with the App's token: delete the labels, close the issues.
for l in $(jq -r '.[] | select(.name | ascii_downcase | startswith("p25app")) | .name' <<<"$BODY"); do
  call "Y-delete-label-$(tr ':' '-' <<<"$l")" app "$V1" DELETE "repos/$R/labels/$l"
done
i=0
for n in $(cat "$OUT/issue-number.txt"); do
  i=$((i + 1))
  call "Y-close-issue-$i" app "$V1" PATCH "repos/$R/issues/$n" '{"state":"closed"}'
done
call Z4-repo-labels-after-cleanup op "$V1" GET "repos/$R/labels?per_page=100"
jq -c '[.[].name]' <<<"$BODY"
