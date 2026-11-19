# PROTOTYPE. Debian stable with the three tools capture_proto.py needs.
FROM debian:stable-slim
RUN apt-get -o Acquire::Retries=8 update -qq \
 && apt-get -o Acquire::Retries=8 install -y -qq --no-install-recommends git python3 bubblewrap \
 && useradd -m proto
