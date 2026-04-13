FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

ARG CLAUDE_CODE_VERSION=2.1.104
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    gnupg \
    bash \
    tini \
    sudo \
  && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/apt/keyrings \
  && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
    | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
  && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_22.x nodistro main" \
    > /etc/apt/sources.list.d/nodesource.list \
  && apt-get update \
  && apt-get install -y --no-install-recommends nodejs \
  && npm install -g @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION} \
  && rm -rf /var/lib/apt/lists/*

COPY docker/claude-runner/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

RUN useradd --system --create-home --home-dir /workspace claude \
  && mkdir -p /workspace /state/claude \
  && chown claude:claude /workspace /state/claude \
  && chown claude:claude /state

RUN cat <<'EOF' > /usr/local/bin/claude-bootstrap.sh
#!/usr/bin/env bash
set -euo pipefail
mkdir -p /workspace /state/claude
chown claude:claude /workspace
chown claude:claude /state/claude
touch /state/claude/.keep
chown claude:claude /state
EOF
RUN chmod +x /usr/local/bin/claude-bootstrap.sh
RUN printf 'claude ALL=(root) NOPASSWD: /usr/local/bin/claude-bootstrap.sh\n' \
    > /etc/sudoers.d/claude \
  && chmod 0440 /etc/sudoers.d/claude

WORKDIR /workspace
USER claude
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/entrypoint.sh"]
