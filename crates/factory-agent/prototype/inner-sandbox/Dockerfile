FROM debian:13
RUN apt-get update && apt-get install -y --no-install-recommends bubblewrap socat curl ca-certificates python3 procps && rm -rf /var/lib/apt/lists/*
RUN useradd -m probe
USER probe
WORKDIR /home/probe
RUN curl -fsSL https://claude.ai/install.sh | bash -s 2.1.311
