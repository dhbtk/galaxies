# syntax=docker/dockerfile:1
FROM rust:1.96-bookworm AS backend-build
RUN apt-get update && apt-get install -y --no-install-recommends clang libclang-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/galaxy-catalog/Cargo.toml crates/galaxy-catalog/Cargo.toml
COPY crates/galaxy-catalog/src crates/galaxy-catalog/src
COPY crates/galaxy-scraper/Cargo.toml crates/galaxy-scraper/Cargo.toml
COPY crates/galaxy-scraper/src crates/galaxy-scraper/src
COPY crates/web/Cargo.toml crates/web/Cargo.toml
COPY crates/web/src crates/web/src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p web && cp target/release/web /web

FROM debian:bookworm-slim AS backend
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-build /web /usr/local/bin/galaxies-web
COPY --chown=10001:10001 crates/web/catalog.sqlite ./
COPY crates/web/catalog-images ./catalog-images
USER 10001:10001
ENV RUST_LOG=info
EXPOSE 3000
CMD ["galaxies-web"]
