#!/usr/bin/env bash
# Reads the responses that paging.sh kept and prints one line for each GraphQL read:
# what was asked, how many nodes came back, the counts, the page information and the
# first and the last node. Throwaway code.
#
# usage: summary.sh <output directory>
set -u
OUT=$1
body_of() { sed '1,/^$/d' "$OUT/$1.txt"; }

for f in "$OUT"/[2-7]*.txt; do
  name=$(basename "$f" .txt)
  case $name in 5*) continue ;; 7a* | 7b* | 7c* | 7k*) continue ;; esac
  body_of "$name" | jq -r --arg name "$name" '
    def who: if . == null then "null" else "\(.login)/\(.__typename)/\(.databaseId)" end;
    def lbl:
      if .diff != null then (.diff | split("\n") | last) + " by " + (.editor | who)
      elif .__typename == "RenamedTitleEvent" then "renamed " + ((.currentTitle // "") | sub("P-8 probe: paging, "; "")) + " by " + (.actor | who)
      elif .__typename == "LabeledEvent" then "labeled by " + (.actor | who)
      elif .__typename == "UnlabeledEvent" then "unlabeled by " + (.actor | who)
      elif .editedAt != null then .editedAt + " by " + (.editor | who)
      elif .body != null then .body + " by " + (.author | who)
      elif .databaseId != null then "comment \(.databaseId) by " + (.author | who)
      else (.__typename // "?") + " " + (.createdAt // "") end;
    if .errors then "\($name): ERROR \(.errors | map((.type // "") + " " + .message) | join("; "))"
    else
      .data.rateLimit as $rl | .data.repository.issue as $i
      | [("userContentEdits", "timelineItems", "comments") as $k | select($i[$k] != null) | {k: $k, c: $i[$k]}]
      | map("\($name) \(.k): nodes \(.c.edges | length), totalCount \(.c.totalCount)"
          + (if .c.filteredCount != null then ", filteredCount \(.c.filteredCount)" else "" end)
          + (if .c.pageCount != null then ", pageCount \(.c.pageCount)" else "" end)
          + ", next \(.c.pageInfo.hasNextPage), previous \(.c.pageInfo.hasPreviousPage), cost \($rl.cost)\n"
          + "    first: \(if (.c.edges | length) > 0 then (.c.edges[0].node | lbl) else "-" end)\n"
          + "    last:  \(if (.c.edges | length) > 0 then (.c.edges[-1].node | lbl) else "-" end)\n"
          + "    actors: \(.c.edges | map(.node | (.editor // .actor // .author) | who) | group_by(.) | map("\(.[0]) x\(length)") | join(", "))")
      | join("\n")
    end'
done

echo
echo "REST"
for f in "$OUT"/5*.txt; do
  name=$(basename "$f" .txt)
  printf '%s: %s events; %s\n' "$name" "$(body_of "$name" | jq 'length')" \
    "$(grep -i '^link:' "$f" | sed -E 's/<[^>]*[?&]page=([0-9]+)[^>]*>; rel="([a-z]+)"/\2=\1/g' || true)"
done

echo
echo "The App's reads against the operator's"
for f in "$OUT"/*-op.txt; do
  name=$(basename "$f" -op.txt)
  [ -f "$OUT/$name-app.txt" ] || continue
  # The rate limit is each token's own, so it is left out of the comparison.
  if diff -q <(body_of "$name-op" | jq -S 'if type == "object" then del(.data.rateLimit) else . end') <(body_of "$name-app" | jq -S 'if type == "object" then del(.data.rateLimit) else . end') >/dev/null; then
    echo "  $name: the same"
  else
    echo "  $name: DIFFERENT"
  fi
done
