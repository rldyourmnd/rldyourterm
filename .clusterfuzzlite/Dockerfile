# gcr.io/oss-fuzz-base/base-builder-rust pinned 2026-03-10
FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:12ce99d6ac9501c9a6452d7bd384d6cb2376788bdfc602a2cd6814f5fd3fa9c9

WORKDIR $SRC
ARG CFLITE_RUST_TOOLCHAIN=nightly-2026-03-11
RUN rustup toolchain install ${CFLITE_RUST_TOOLCHAIN} --profile minimal --component rust-src \
  && rustup default ${CFLITE_RUST_TOOLCHAIN}
ENV CFLITE_RUST_TOOLCHAIN=${CFLITE_RUST_TOOLCHAIN}
ENV RUSTUP_TOOLCHAIN=${CFLITE_RUST_TOOLCHAIN}
COPY .clusterfuzzlite/build.sh $SRC/build.sh
COPY .clusterfuzzlite/project.yaml $SRC/project.yaml
RUN chmod +x $SRC/build.sh
COPY . $SRC/rldyourterm
