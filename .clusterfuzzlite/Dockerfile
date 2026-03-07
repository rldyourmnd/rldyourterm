FROM gcr.io/oss-fuzz-base/base-builder-rust

WORKDIR $SRC
COPY .clusterfuzzlite/build.sh $SRC/build.sh
COPY .clusterfuzzlite/project.yaml $SRC/project.yaml
RUN chmod +x $SRC/build.sh
COPY . $SRC/rldyourterm
