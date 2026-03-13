FROM jenkins/inbound-agent:latest-jdk21@sha256:8cdeac9601a3381a629555e8d191f1bc4a7ee17546bbf64de54dabde9d2aacd9

USER root

ENV DEBIAN_FRONTEND=noninteractive
ENV CARGO_HOME=/opt/rust/cargo
ENV RUSTUP_HOME=/opt/rust/rustup
ENV PATH=/opt/rust/cargo/bin:/opt/codeql:${PATH}

ARG RUSTUP_VERSION=1.29.0
ARG RUSTUP_INIT_SHA256=4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10
ARG CODEQL_BUNDLE_VERSION=2.24.3
ARG CODEQL_BUNDLE_SHA256=9bfaf079ed2e132a26d9352a0bc1a70c5de0b63131d39e8350d890fbea254b71
ARG SCORECARD_VERSION=5.4.0
ARG SCORECARD_SHA256=e5183aeaa5aa548fbb7318a6deb3e1038be0ef9aca24e655422ae88dfbe67502
ARG CARGO_MACHETE_VERSION=0.9.1
ARG CARGO_FUZZ_VERSION=0.13.1
ARG CARGO_UDEPS_VERSION=0.1.60
ARG RUST_FUZZ_TOOLCHAIN=nightly-2026-03-11
ARG JENKINS_UID=1001
ARG JENKINS_GID=1001

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
        pkg-config \
        python3 \
        python3-pip \
        unzip \
        xz-utils \
        zsh \
    && rm -rf /var/lib/apt/lists/*

RUN groupmod --gid ${JENKINS_GID} jenkins \
    && usermod --uid ${JENKINS_UID} --gid ${JENKINS_GID} jenkins \
    && mkdir -p /opt/rust /opt/codeql /home/jenkins/agent

RUN curl -fsSLo /tmp/rustup-init "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/x86_64-unknown-linux-gnu/rustup-init" \
    && echo "${RUSTUP_INIT_SHA256} */tmp/rustup-init" | sha256sum -c - \
    && chmod +x /tmp/rustup-init \
    && /tmp/rustup-init -y --profile minimal --default-toolchain 1.94.0 \
    && rustup toolchain install 1.92.0 --profile minimal \
    && rustup toolchain install ${RUST_FUZZ_TOOLCHAIN} --profile minimal \
    && rustup component add clippy rustfmt --toolchain 1.94.0 \
    && rustup component add rust-src rustc-dev llvm-tools-preview --toolchain ${RUST_FUZZ_TOOLCHAIN} \
    && cargo install --locked cargo-audit cargo-deny cargo-machete@${CARGO_MACHETE_VERSION} cargo-fuzz@${CARGO_FUZZ_VERSION} \
    && cargo +${RUST_FUZZ_TOOLCHAIN} install --locked cargo-udeps@${CARGO_UDEPS_VERSION} \
    && curl -fsSLo /tmp/codeql-bundle-linux64.tar.gz "https://github.com/github/codeql-action/releases/download/codeql-bundle-v${CODEQL_BUNDLE_VERSION}/codeql-bundle-linux64.tar.gz" \
    && echo "${CODEQL_BUNDLE_SHA256} */tmp/codeql-bundle-linux64.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/codeql-bundle-linux64.tar.gz -C /opt \
    && ln -sf /opt/codeql/codeql /usr/local/bin/codeql \
    && curl -fsSLo /tmp/scorecard_linux_amd64.tar.gz "https://github.com/ossf/scorecard/releases/download/v${SCORECARD_VERSION}/scorecard_${SCORECARD_VERSION}_linux_amd64.tar.gz" \
    && echo "${SCORECARD_SHA256} */tmp/scorecard_linux_amd64.tar.gz" | sha256sum -c - \
    && tar -xzf /tmp/scorecard_linux_amd64.tar.gz -C /usr/local/bin scorecard \
    && rm -f /tmp/rustup-init /tmp/codeql-bundle-linux64.tar.gz /tmp/scorecard_linux_amd64.tar.gz \
    && chown -R jenkins:jenkins /opt/rust /opt/codeql

COPY agents/rust-linux-ci/entrypoint.sh /usr/local/bin/jenkins-agent-entrypoint
COPY controller/support/ /opt/jenkins/support/
RUN chmod +x /usr/local/bin/jenkins-agent-entrypoint /opt/jenkins/support/*.sh \
    && chown -R jenkins:jenkins /opt/jenkins/support

USER jenkins
WORKDIR /home/jenkins

ENTRYPOINT ["/usr/local/bin/jenkins-agent-entrypoint"]
