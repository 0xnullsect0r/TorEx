# syntax=docker/dockerfile:1
FROM alpine:3.21

# BuildKit cache mount keeps APKINDEX persistent across builds (never re-fetched).
# Retry loop handles transient CDN failures on first fetch.
RUN --mount=type=cache,target=/var/cache/apk \
    for i in 1 2 3 4 5; do apk update && break || (echo "Retry $i..." && sleep $((i*5))); done \
    && apk add \
        tor \
        build-base \
        autoconf \
        automake \
        libsodium-dev \
        git

# Build mkp224o then remove build tools (separate layer so mkp224o build is cached)
RUN git clone --depth=1 https://github.com/cathugger/mkp224o /tmp/mkp224o \
    && cd /tmp/mkp224o \
    && ./autogen.sh \
    && ./configure \
    && make -j$(nproc) \
    && strip mkp224o \
    && cp mkp224o /usr/local/bin/mkp224o \
    && rm -rf /tmp/mkp224o \
    && apk del build-base autoconf automake libsodium-dev git

COPY torrc /etc/tor/torrc
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

VOLUME ["/var/lib/tor"]
ENTRYPOINT ["/entrypoint.sh"]
