# Build with the repository root as context:
#   docker build -f services/compressor/Dockerfile .

FROM rust:1.90-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    clang cmake protobuf-compiler && rm -rf /var/lib/apt/lists/*

# Cap parallel rustc jobs so release builds fit in the default Docker Desktop
# memory allocation; override with --build-arg CARGO_BUILD_JOBS=<n> on big VMs.
ARG CARGO_BUILD_JOBS=4
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY proto ./proto
COPY crates ./crates
COPY services/compressor ./services/compressor
COPY services/polyphony ./services/polyphony
COPY services/mapi ./services/mapi
COPY services/tikv-gc ./services/tikv-gc
COPY services/daw ./services/daw

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo build --release -p compressor \
    && cp target/release/compressor /compressor

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl tini socat && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --create-home nonroot \
    && mkdir -p /data && chown nonroot:nonroot /data

COPY --from=builder /compressor /usr/local/bin/compressor

USER nonroot
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "compressor"]
