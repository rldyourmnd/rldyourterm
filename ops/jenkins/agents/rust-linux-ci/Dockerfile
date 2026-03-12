FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/opt/rust/cargo
ENV RUSTUP_HOME=/opt/rust/rustup
ENV PATH=/opt/rust/cargo/bin:/opt/codeql:${PATH}

ARG CODEQL_BUNDLE_VERSION=2.24.3
ARG SCORECARD_VERSION=5.4.0
ARG CARGO_FUZZ_VERSION=0.13.1
ARG RUST_FUZZ_TOOLCHAIN=nightly-2026-03-11

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        build-essential \
        ca-certificates \
        curl \
        fish \
        git \
        jq \
        libssl-dev \
        libwayland-dev \
        libxkbcommon-dev \
        openjdk-21-jre-headless \
        pkg-config \
        python3 \
        python3-pip \
        unzip \
        xz-utils \
        zsh \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --shell /bin/bash jenkins \
    && mkdir -p /opt/rust /opt/codeql /home/jenkins/agent \
    && chown -R jenkins:jenkins /opt/rust /opt/codeql /home/jenkins

RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.94.0 \
    && rustup toolchain install 1.92.0 --profile minimal \
    && rustup toolchain install ${RUST_FUZZ_TOOLCHAIN} --profile minimal \
    && rustup component add clippy rustfmt --toolchain 1.94.0 \
    && cargo install --locked cargo-audit cargo-deny cargo-fuzz@${CARGO_FUZZ_VERSION} \
    && curl -fsSL "https://github.com/github/codeql-action/releases/download/codeql-bundle-v${CODEQL_BUNDLE_VERSION}/codeql-bundle-linux64.tar.gz" \
      | tar -xz -C /opt \
    && ln -sf /opt/codeql/codeql /usr/local/bin/codeql \
    && curl -fsSL "https://github.com/ossf/scorecard/releases/download/v${SCORECARD_VERSION}/scorecard_${SCORECARD_VERSION}_linux_amd64.tar.gz" \
      | tar -xz -C /usr/local/bin scorecard \
    && chown -R jenkins:jenkins /opt/rust /opt/codeql

COPY agents/rust-linux-ci/entrypoint.sh /usr/local/bin/jenkins-agent-entrypoint
COPY controller/support/ /opt/jenkins/support/
RUN chmod +x /usr/local/bin/jenkins-agent-entrypoint /opt/jenkins/support/*.sh \
    && chown -R jenkins:jenkins /opt/jenkins/support

USER jenkins
WORKDIR /home/jenkins

ENTRYPOINT ["/usr/local/bin/jenkins-agent-entrypoint"]
