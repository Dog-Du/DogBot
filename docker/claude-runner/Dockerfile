FROM ubuntu:24.04

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    git \
    unzip \
    nano \
    gnupg \
    build-essential \
    lsb-release \
  && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
  && apt-get install -y nodejs \
  && npm install -g npm@10

RUN curl -fsSL https://claude-cli.example.com/install.sh | sh

RUN mkdir -p /workspace /state
WORKDIR /workspace

COPY docker/claude-runner/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

CMD ["claude"]
