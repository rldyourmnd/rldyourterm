FROM gcr.io/oss-fuzz-base/base-builder-rust

WORKDIR $SRC
RUN rustup toolchain install nightly --profile minimal --component rust-src && rustup default nightly
ENV RUSTUP_TOOLCHAIN=nightly
COPY .clusterfuzzlite/build.sh $SRC/build.sh
COPY .clusterfuzzlite/project.yaml $SRC/project.yaml
RUN chmod +x $SRC/build.sh
COPY . $SRC/rldyourterm
