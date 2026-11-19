#!/bin/sh
# PROTOTYPE. Runs capture_proto.py on Debian stable in Docker as an unprivileged user.
#   ./linux.sh                     no sandbox, so a fired canary is visible
#   SF_PROTO_BWRAP=1 ./linux.sh    the staging git runs under bubblewrap
# The container is privileged only so that bubblewrap can create its namespaces (P-16).
set -e
here=$(cd "$(dirname "$0")" && pwd)
docker build -q -t sf-p7-proto "$here" >/dev/null
exec docker run --rm --privileged -e SF_PROTO_BWRAP="${SF_PROTO_BWRAP:-0}" -e SF_PROTO_TMP=/home/proto \
  -v "$here":/proto:ro -u proto -w /home/proto sf-p7-proto \
  sh -c 'echo "Debian $(cat /etc/debian_version), $(bwrap --version)"; exec python3 /proto/capture_proto.py "$@"' sh "$@"
