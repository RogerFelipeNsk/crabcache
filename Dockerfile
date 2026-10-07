# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked \
 && cp target/release/crabcache /crabcache \
 && strip /crabcache

# Distroless: no shell or package manager, runs as an unprivileged user.
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /crabcache /usr/local/bin/crabcache
# Listen on all interfaces inside the container. Set CRABCACHE_REQUIREPASS when the port is reachable
# from outside the host.
ENV CRABCACHE_BIND=0.0.0.0
EXPOSE 6379
USER nonroot
ENTRYPOINT ["/usr/local/bin/crabcache"]
