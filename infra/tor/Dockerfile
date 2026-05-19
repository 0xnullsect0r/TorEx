# Stage 1: compile mkp224o on Alpine (apk is more reliable than apt in CI/Docker)
FROM alpine:3.19 AS mkp224o-builder

RUN apk add --no-cache \
    build-base \
    autoconf \
    automake \
    libsodium-dev \
    git

RUN git clone --depth=1 https://github.com/cathugger/mkp224o /tmp/mkp224o \
    && cd /tmp/mkp224o \
    && ./autogen.sh \
    && ./configure --enable-static-sodium \
    && make -j$(nproc) \
    && strip mkp224o \
    && cp mkp224o /usr/local/bin/mkp224o

# Stage 2: runtime — Alpine tor + statically linked mkp224o binary
FROM alpine:3.19

RUN apk add --no-cache tor bash

COPY --from=mkp224o-builder /usr/local/bin/mkp224o /usr/local/bin/mkp224o
COPY torrc /etc/tor/torrc
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

VOLUME ["/var/lib/tor"]
ENTRYPOINT ["/entrypoint.sh"]
