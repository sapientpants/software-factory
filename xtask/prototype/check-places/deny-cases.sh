#!/bin/bash
# What cargo deny does without the advisory database and without a network
# (P-38, for issue 87). Runs on macOS: `sandbox-exec` takes the network away.
#
#   deny-cases.sh <cargo-deny binary> <a copy of the workspace> <scratch directory>
#
# The copy holds a deny.toml that allows the licences of the tree, so that a
# failure is the advisory database's and nothing else's.
set -u
D=$1; W=$2; O=$3
export CARGO_HOME=$O/cargo-home
DB=$CARGO_HOME/advisory-dbs
NAME=advisory-db-3157b0e258782691
nonet() { sandbox-exec -p '(version 1)(allow default)(deny network*)' "$@"; }
run() {
  echo; echo "\$ $*" | sed -e "s#$D#cargo-deny#g" -e "s#$O#<scratch>#g"
  "$@" 2>&1 | grep -v '^mise' | sed -e "s#$O#<scratch>#g" -e "s#$W#<workspace>#g" | tail -12
  echo "(exit ${PIPESTATUS[0]})"
}
licences() { printf '[licenses]\nallow = ["MIT", "Apache-2.0"]\n'; }
rm -rf "$CARGO_HOME" "$O/elsewhere" "$O/plain"; cd "$W"; licences > deny.toml

echo "## versions"; "$D" --version; git --version; sw_vers | tr '\n' ' '; echo
echo; echo "## 1. the dependencies, fetched with a network"; run cargo fetch --locked
echo; echo "## 2. --disable-fetch"; run "$D" check --disable-fetch
echo; echo "## 3. no database, no network"
run nonet "$D" --offline check
run nonet "$D" check
run nonet "$D" --offline check bans licenses sources
echo; echo "## 4. the fetch, and where the database lies"; run "$D" fetch db
(cd "$CARGO_HOME" && ls -1 advisory-dbs advisory-dbs/$NAME/.git | tr '\n' ' '; echo)
echo; echo "## 5. the database present, no network"
run nonet "$D" --locked --offline check
run nonet "$D" --frozen check advisories
run nonet "$D" check advisories
echo "FETCH_HEAD after the fetch that failed: $(wc -c < "$DB/$NAME/.git/FETCH_HEAD" | tr -d ' ') bytes"
echo; echo "## 6. a root directory that cannot be written"
chmod -R a-w "$DB"; run nonet "$D" --offline check advisories; chmod -R u+w "$DB"
echo; echo "## 7. a writable root without db.lock, the database read-only"
rm -f "$DB/db.lock"; chmod -R a-w "$DB/$NAME"; run nonet "$D" --offline check advisories
ls -1 "$DB" | tr '\n' ' '; echo
chmod u+w "$DB/$NAME/.git"; rm -f "$DB/$NAME/.git/FETCH_HEAD"; chmod a-w "$DB/$NAME/.git"
echo "-- the same without FETCH_HEAD"; run nonet "$D" --offline check advisories; chmod -R u+w "$DB"
echo; echo "## 8. the age of the last fetch"
touch "$DB/$NAME/.git/FETCH_HEAD"
touch -t "$(date -v-89d +%Y%m%d%H%M)" "$DB/$NAME/.git/FETCH_HEAD"; echo "-- FETCH_HEAD 89 days old, the default limit"; run nonet "$D" --offline check advisories
touch -t "$(date -v-91d +%Y%m%d%H%M)" "$DB/$NAME/.git/FETCH_HEAD"; echo "-- 91 days old"; run nonet "$D" --offline check advisories
{ licences; printf '[advisories]\nmaximum-db-staleness = "P7D"\n'; } > deny.toml
touch -t "$(date -v-6d +%Y%m%d%H%M)" "$DB/$NAME/.git/FETCH_HEAD"; echo "-- limit P7D, 6 days old"; run nonet "$D" --offline check advisories
touch -t "$(date -v-8d +%Y%m%d%H%M)" "$DB/$NAME/.git/FETCH_HEAD"; echo "-- limit P7D, 8 days old"; run nonet "$D" --offline check advisories
echo "-- then a fetch that fails, and the check again"; nonet "$D" fetch db >/dev/null 2>&1; echo "(fetch exit $?)"; run nonet "$D" --offline check advisories
rm -f "$DB/$NAME/.git/FETCH_HEAD"; touch -t "$(date -v-8d +%Y%m%d%H%M)" "$DB/$NAME/.git/HEAD"
echo "-- no FETCH_HEAD, HEAD 8 days old, the commit of HEAD: $(git -C "$DB/$NAME" show -s --format=%cI HEAD)"; run nonet "$D" --offline check advisories
echo; echo "## 9. db-path from a variable"
mkdir -p "$O/elsewhere"; cp -R "$DB/$NAME" "$O/elsewhere/"; touch "$O/elsewhere/$NAME/.git/FETCH_HEAD"
{ licences; printf '[advisories]\ndb-path = "$SF_P38/elsewhere"\n'; } > deny.toml
SF_P38=$O run nonet "$D" --offline check advisories
run nonet "$D" --offline check advisories
echo; echo "## 10. a clone made by git under the directory name"
mkdir -p "$O/plain"; git clone -q --depth=1 https://github.com/RustSec/advisory-db "$O/plain/$NAME"
{ licences; printf '[advisories]\ndb-path = "%s/plain"\n' "$O"; } > deny.toml
run nonet "$D" --offline check advisories
licences > deny.toml
