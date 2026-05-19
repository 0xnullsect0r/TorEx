# Stage 1: compile mkp224o with static libsodium (no runtime .so needed)
FROM debian:bookworm-slim AS mkp224o-builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    autoconf \
    automake \
    libsodium-dev \
    git \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN git clone --depth=1 https://github.com/cathugger/mkp224o /tmp/mkp224o \
    && cd /tmp/mkp224o \
    && ./autogen.sh \
    && ./configure --enable-static-sodium \
    && make -j$(nproc) \
    && strip mkp224o \
    && cp mkp224o /usr/local/bin/mkp224o

# Stage 2: runtime — tor only; mkp224o is statically linked, no libsodium needed
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    tor \
    && rm -rf /var/lib/apt/lists/*

COPY --from=mkp224o-builder /usr/local/bin/mkp224o /usr/local/bin/mkp224o
COPY torrc /etc/tor/torrc
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

VOLUME ["/var/lib/tor"]
ENTRYPOINT ["/entrypoint.sh"]
