FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright \
    PLAYWRIGHT_SKIP_BROWSER_GC=1 \
    XDG_CACHE_HOME=/state/browser-cache \
    BROWSER_PROFILE_DIR=/state/browser-profile \
    BROWSER_DOWNLOADS_DIR=/workspace/downloads

ARG CLAUDE_CODE_VERSION=2.1.104
ARG PLAYWRIGHT_CLI_VERSION=0.1.8
ARG PLAYWRIGHT_MCP_VERSION=0.0.70
ARG BIFROST_VERSION=1.6.2
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    bash \
    curl \
    wget \
    aria2 \
    git \
    git-lfs \
    openssh-client \
    gnupg \
    sudo \
    tini \
    less \
    file \
    tree \
    coreutils \
    findutils \
    grep \
    sed \
    gawk \
    diffutils \
    patch \
    procps \
    psmisc \
    lsof \
    strace \
    rsync \
    ripgrep \
    fd-find \
    jq \
    miller \
    sqlite3 \
    csvkit \
    zip \
    unzip \
    xz-utils \
    p7zip-full \
    dnsutils \
    iputils-ping \
    netcat-openbsd \
    inetutils-telnet \
    openssl \
    build-essential \
    make \
    pkgconf \
    python-is-python3 \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    poppler-utils \
    pandoc \
    antiword \
    catdoc \
    ffmpeg \
    tesseract-ocr \
    imagemagick \
    libxml2-utils \
    xvfb \
    w3m \
    xdg-utils \
    fonts-dejavu-core \
    fonts-liberation \
    fonts-noto-cjk \
    fonts-noto-color-emoji \
    vim

RUN mkdir -p /etc/apt/keyrings \
  && curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
    | gpg --dearmor -o /etc/apt/keyrings/nodesource.gpg \
  && echo "deb [signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_22.x nodistro main" \
    > /etc/apt/sources.list.d/nodesource.list \
  && apt-get update \
  && apt-get install -y --no-install-recommends nodejs \
  && npm install -g \
    @anthropic-ai/claude-code@${CLAUDE_CODE_VERSION} \
    @maximhq/bifrost@${BIFROST_VERSION} \
    @playwright/cli@${PLAYWRIGHT_CLI_VERSION} \
    @playwright/mcp@${PLAYWRIGHT_MCP_VERSION} \
  && corepack enable \
  && npx playwright install --with-deps chromium \
  && ln -sf /usr/bin/fdfind /usr/local/bin/fd

COPY deploy/docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

RUN useradd --system --create-home --home-dir /workspace claude \
  && mkdir -p \
    /ms-playwright \
    /workspace \
    /workspace/downloads \
    /state/claude \
    /state/bifrost \
    /state/browser-profile \
    /state/browser-cache \
  && chown -R claude:claude /ms-playwright \
  && chown claude:claude /workspace /workspace/downloads /state/claude /state/bifrost /state/browser-profile /state/browser-cache \
  && chown claude:claude /state

RUN printf '%s\n' \
    '#!/usr/bin/env bash' \
    'set -euo pipefail' \
    'mkdir -p /workspace /state /state/claude /state/claude/debug /state/bifrost' \
    'mkdir -p /workspace/downloads /state/browser-profile /state/browser-cache' \
    'chown claude:claude /workspace /workspace/downloads' \
    'chown claude:claude /state' \
    'chown -R claude:claude /state/claude' \
    'chown -R claude:claude /state/bifrost' \
    'chown -R claude:claude /state/browser-profile /state/browser-cache' \
    'touch /state/claude/.keep' \
    'chown claude:claude /state/claude/.keep' \
    > /usr/local/bin/claude-bootstrap.sh \
  && chmod +x /usr/local/bin/claude-bootstrap.sh
RUN printf 'claude ALL=(ALL:ALL) NOPASSWD: ALL\n' \
    > /etc/sudoers.d/claude \
  && chmod 0440 /etc/sudoers.d/claude

WORKDIR /workspace
USER claude
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["/usr/local/bin/entrypoint.sh"]
