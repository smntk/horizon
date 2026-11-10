# syntax=docker/dockerfile:1.7
# Horizon dedicated server with the mods in server-mods/ (packaging/coolify/README.md). Build context: the
# repository root. docker-compose.yml at the root builds it; Coolify reads that file.
#
# The image holds none of Minecraft's files: mount the prepared 26.3 folder (horizon-prepare) read-only at
# /opt/horizon/.local/horizon-client/26.3.
FROM rust:1-slim-bookworm AS build
RUN apt-get update \
    && apt-get install -y --no-install-recommends git rsync unzip \
    && rm -rf /var/lib/apt/lists/*
ARG PROFILE=release
ARG MIXINS_REV=5a64af938ec86b00d53d16a408aabc0038fb3e57
ARG HORIZON_REVISION=
WORKDIR /src
COPY . .
# The cargo registry, the target folder and a mirror of the sources are cached between builds, so a change
# recompiles only what it touches instead of the whole game (build.sh: the mirror keeps cargo's timestamps).
RUN --mount=type=cache,id=horizon-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=horizon-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=horizon-target,target=/target \
    --mount=type=cache,id=horizon-src-mirror,target=/srcmirror \
    PROFILE="$PROFILE" MIXINS_REV="$MIXINS_REV" HORIZON_REVISION="$HORIZON_REVISION" \
    sh -c "sed 's/\r\$//' packaging/coolify/build.sh | sh"

FROM debian:bookworm-slim
RUN useradd --system --home-dir /data --shell /usr/sbin/nologin horizon \
    && mkdir -p /data /opt/horizon/.local/horizon-client \
    && chown horizon:horizon /data
COPY --from=build /out/ /opt/horizon/
COPY packaging/coolify/entrypoint.sh /opt/horizon/entrypoint.sh
RUN sed -i 's/\r$//' /opt/horizon/entrypoint.sh && chmod 755 /opt/horizon/entrypoint.sh
ENV PATH=/opt/horizon:$PATH \
    HORIZON_NO_AUDIO=1 \
    HORIZON_NO_UPDATE_CHECK=1
WORKDIR /data
VOLUME /data
EXPOSE 25565
USER horizon
# SIGTERM makes horizon-server save and stop the game: the compose file gives it 180 s.
CMD ["/opt/horizon/entrypoint.sh"]
