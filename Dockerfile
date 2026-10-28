# `atk serve`: ATK's tools over HTTP, the container a dashboard such as truebrew
# runs workflows through. See "Dashboard service" in the README.
#
# Jobs run in sibling containers on the host's Docker, through the mounted socket.
# Their workspaces are bind-mounted by path, so the folder ATK copies them into
# (TMPDIR) must be mounted at the same path inside and out: `-v /tmp/atk:/tmp/atk`.
FROM rust:1.88-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
RUN cargo build --release --bin atk

FROM docker:27-cli AS docker

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates git tini \
    && rm -rf /var/lib/apt/lists/*
COPY --from=docker /usr/local/bin/docker /usr/local/bin/docker
COPY --from=build /src/target/release/atk /usr/local/bin/atk
# Run history and settings live in /data; projects are the folders in /projects.
ENV XDG_DATA_HOME=/data XDG_CONFIG_HOME=/data/config TMPDIR=/tmp/atk
VOLUME /data
EXPOSE 8080
# tini collects the background runs once they finish.
ENTRYPOINT ["tini", "--", "atk", "serve", "--listen", "0.0.0.0:8080", "--projects", "/projects", "--key-file", "/run/truebrew/key"]
