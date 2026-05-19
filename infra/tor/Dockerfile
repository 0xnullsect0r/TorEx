FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    tor \
    build-essential \
    libsodium-dev \
    git \
    ca-certificates \
    && git clone --depth=1 https://github.com/cathugger/mkp224o /tmp/mkp224o \
    && cd /tmp/mkp224o \
    && ./autogen.sh && ./configure && make -j$(nproc) \
    && cp mkp224o /usr/local/bin/mkp224o \
    && rm -rf /tmp/mkp224o \
    && rm -rf /var/lib/apt/lists/*

COPY torrc /etc/tor/torrc
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

VOLUME ["/var/lib/tor"]
ENTRYPOINT ["/entrypoint.sh"]
