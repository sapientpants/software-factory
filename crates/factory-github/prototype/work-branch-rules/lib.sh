# Common parts of the probes of #82: P-5, P-1, P-8 and P-25 under the scratch App's
# installation token. Sourced by the probe scripts, not run. Throwaway code.
#
# Runs against the scratch repository only. The App's private key is read by openssl
# alone. A token lives in a shell variable for the length of one script and is revoked
# at its end; nothing here writes one to a file.

OWNER=sapientpants
REPO=sf-scratch
R=$OWNER/$REPO
API=https://api.github.com
APP_ID=5926866
INSTALLATION_ID=185898834
KEY=${SF_SCRATCH_APP_KEY:-$HOME/Projects/sf-scratch-app/sf-scratch-app.2026-11-25.private-key.pem}
V1=2026-03-10 # the version GH-17 pins
V2=2022-11-28

b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }

# app_jwt: a JSON Web Token for the App, valid for five minutes.
app_jwt() {
  local now header payload sig
  now=$(date +%s)
  header=$(printf '{"alg":"RS256","typ":"JWT"}' | b64url)
  payload=$(printf '{"iat":%d,"exp":%d,"iss":"%s"}' $((now - 60)) $((now + 300)) "$APP_ID" | b64url)
  sig=$(printf '%s.%s' "$header" "$payload" | openssl dgst -sha256 -sign "$KEY" -binary | b64url)
  printf '%s.%s.%s' "$header" "$payload" "$sig"
}

# mint_token: sets TOKEN to a new installation token, restricted to the scratch
# repository as GH-12 has it. The response is kept without the token.
mint_token() {
  local resp
  resp=$(curl -sS -X POST "$API/app/installations/$INSTALLATION_ID/access_tokens" \
    -H "Authorization: Bearer $(app_jwt)" -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: $V1" -d "{\"repositories\":[\"$REPO\"]}")
  TOKEN=$(jq -r '.token // empty' <<<"$resp")
  if [ -z "$TOKEN" ]; then
    echo "no installation token: $(jq -c 'del(.token)' <<<"$resp")" >&2
    exit 1
  fi
  jq '.token = "<removed>" | .repositories |= map({id, full_name, private})' <<<"$resp" >"$OUT/00-token.json"
  export TOKEN
  trap revoke_token EXIT
}

revoke_token() {
  local code
  code=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE "$API/installation/token" \
    -H "Authorization: Bearer $TOKEN" -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: $V1")
  echo "token revoked: $code"
}

# The operator's token, as gh holds it: for setup, for cleanup and for the reads that
# the App's are set against.
op_token() {
  OP_TOKEN=$(gh auth token)
  export OP_TOKEN
}

# call <name> <app|op|none> <version> <method> <path> [body]
# One request. Kept in $OUT/<name>.txt: the status line, four headers, a blank line, the body.
# BODY holds the body afterwards and STATUS the status code.
call() {
  local name=$1 who=$2 v=$3 method=$4 path=$5
  local auth=() data=() tmp
  tmp=$(mktemp -d)
  case $who in
    app) auth=(-H "Authorization: Bearer $TOKEN") ;;
    op) auth=(-H "Authorization: Bearer $OP_TOKEN") ;;
    none) ;;
  esac
  if [ $# -ge 6 ]; then data=(--data-binary "$6"); fi
  curl -sS -D "$tmp/h" -o "$tmp/b" -X "$method" "$API/$path" ${auth[@]+"${auth[@]}"} \
    -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: $v" ${data[@]+"${data[@]}"}
  {
    grep -iE '^(HTTP/|date:|x-github-api-version-selected:|x-github-request-id:|x-accepted-github-permissions:)' "$tmp/h" | tr -d '\r'
    echo
    cat "$tmp/b"
    echo
  } >"$OUT/$name.txt"
  BODY=$(cat "$tmp/b")
  STATUS=$(head -1 "$tmp/h" | tr -d '\r' | awk '{print $2}')
  printf '%-40s %-4s %-3s %s\n' "$name" "$who" "$STATUS" \
    "$(jq -r 'if type == "object" then (.message // "") else "" end' "$tmp/b" 2>/dev/null | head -1 | cut -c1-110)"
  rm -rf "$tmp"
  case $method in GET) ;; *) sleep 1 ;; esac
}

# graphql <name> <app|op> <query> [variables as JSON]
graphql() {
  local name=$1 who=$2 query=$3 vars=${4:-'{}'}
  call "$name" "$who" "$V1" POST graphql "$(jq -n --arg q "$query" --argjson v "$vars" '{query: $q, variables: $v}')"
}

# body_of <name>: the body of a kept response.
body_of() { sed '1,/^$/d' "$OUT/$1.txt"; }
