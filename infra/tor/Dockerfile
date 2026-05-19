FROM alpine:3.19

# Install tor + build deps in one layer, build mkp224o, then remove build deps
RUN apk add --no-cache \
    tor \
    build-base \
    autoconf \
    automake \
    libsodium-dev \
    git \
    && git clone --depth=1 https://github.com/cathugger/mkp224o /tmp/mkp224o \
    && cd /tmp/mkp224o \
    && ./autogen.sh \
    && ./configure --enable-static-sodium \
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
