FROM gcr.io/oss-fuzz-base/base-builder-rust@sha256:12ce99d6ac9501c9a6452d7bd384d6cb2376788bdfc602a2cd6814f5fd3fa9c9

WORKDIR $SRC
RUN rustup toolchain install nightly --profile minimal --component rust-src && rustup default nightly
ENV RUSTUP_TOOLCHAIN=nightly
COPY .clusterfuzzlite/build.sh $SRC/build.sh
COPY .clusterfuzzlite/project.yaml $SRC/project.yaml
RUN chmod +x $SRC/build.sh
COPY . $SRC/rldyourterm
