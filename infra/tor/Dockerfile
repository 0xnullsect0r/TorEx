# syntax=docker/dockerfile:1
FROM alpine:3.21

# dl-cdn.alpinelinux.org is geo-routed and can be unreliable on some servers.
# Try non-CDN mirrors in order: kernel.org → official dl-4 → CDN fallback.
# BuildKit cache mount persists the APKINDEX across builds once fetched.
RUN --mount=type=cache,target=/var/cache/apk \
    for MIRROR in \
        "https://mirrors.edge.kernel.org/alpine" \
        "https://dl-4.alpinelinux.org/alpine" \
        "https://dl-cdn.alpinelinux.org/alpine"; \
    do \
        printf '%s/v3.21/main\n%s/v3.21/community\n' "$MIRROR" "$MIRROR" > /etc/apk/repositories \
        && apk update \
        && break \
        || echo "Mirror $MIRROR failed, trying next..."; \
    done \
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
