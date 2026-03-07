FROM gcr.io/oss-fuzz-base/base-builder-rust

WORKDIR $SRC
RUN rustup toolchain install nightly --profile minimal && rustup default nightly
COPY .clusterfuzzlite/build.sh $SRC/build.sh
COPY .clusterfuzzlite/project.yaml $SRC/project.yaml
RUN chmod +x $SRC/build.sh
COPY . $SRC/rldyourterm
