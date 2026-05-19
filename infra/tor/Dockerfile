# syntax=docker/dockerfile:1
FROM alpine:3.21

# Alpine base image has no ca-certificates, so HTTPS to external repos fails
# from inside Docker. HTTP is safe: apk verifies packages via GPG signing keys.
RUN --mount=type=cache,id=apk-tor,target=/var/cache/apk \
    printf 'http://dl-cdn.alpinelinux.org/alpine/v3.21/main\nhttp://dl-cdn.alpinelinux.org/alpine/v3.21/community\n' > /etc/apk/repositories \
    && apk update \
    && apk add \
        tor \
        libsodium \
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
