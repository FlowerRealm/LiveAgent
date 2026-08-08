# backend: Rust 后端 + Node 引擎（core），无 WebUI。
# backend spawn core 为子进程，所以它们必须在同一个容器里。
#
#   docker build -f docker/backend.Dockerfile -t liveagent-backend .

FROM node:22.19.0-bookworm-slim AS engine-builder

WORKDIR /src/crates/core

RUN npm install -g pnpm@10.32.1

COPY crates/core/package.json crates/core/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY crates/core ./
RUN pnpm build

FROM rust:1-bookworm AS backend-builder

WORKDIR /src

RUN apt-get update && apt-get install -y --no-install-recommends libclang-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates/backend ./crates/backend
COPY crates/frontend ./crates/frontend

RUN cargo fetch --manifest-path crates/backend/Cargo.toml
RUN cargo build -p backend --release --target-dir /out/target

FROM node:22.19.0-bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && useradd --system --uid 10001 --user-group --home-dir /var/lib/liveagent --shell /usr/sbin/nologin liveagent \
    && install -d -o liveagent -g liveagent -m 0700 /opt/liveagent/engine /var/lib/liveagent \
    && rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /out/target/release/backend /usr/local/bin/backend
COPY --from=engine-builder /src/crates/core/dist/index.js /opt/liveagent/engine/index.js

RUN chown -R liveagent:liveagent /opt/liveagent

USER liveagent

ENV LIVEAGENT_DATA_DIR=/var/lib/liveagent \
    LIVEAGENT_ENGINE_BUNDLE=/opt/liveagent/engine \
    HOME=/var/lib/liveagent

VOLUME ["/var/lib/liveagent"]
EXPOSE 8443

ENTRYPOINT ["/usr/local/bin/backend"]
