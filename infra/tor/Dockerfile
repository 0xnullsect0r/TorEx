FROM alpine:3.21

# Split apk update into its own layer so Docker's layer cache preserves the APKINDEX.
# Subsequent builds reuse this cached layer without hitting the network.
RUN apk update
RUN apk add \
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
