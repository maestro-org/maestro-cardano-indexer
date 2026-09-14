# postgres with the pg_bech32 extension, which the Koios SQL layer (grest)
# uses to render bech32 pool and stake identifiers.
FROM postgres:15 AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential postgresql-server-dev-15 pkg-config git ca-certificates \
    autoconf autoconf-archive automake libtool \
    && rm -rf /var/lib/apt/lists/*

# libbech32: C library pg_bech32 links against
ARG LIBBECH32_REV=v1.1p1
RUN git clone https://github.com/whitslack/libbech32 /src/libbech32 \
    && cd /src/libbech32 \
    && git checkout ${LIBBECH32_REV} \
    && autoreconf -i \
    && ./configure \
    && make \
    && make install \
    && ldconfig

ARG PG_BECH32_REV=c3444aaf11228841770dac0db580bfa01d008e2f
RUN git clone https://github.com/cardano-community/pg_bech32 /src/pg_bech32 \
    && cd /src/pg_bech32 \
    && git checkout ${PG_BECH32_REV} \
    && make \
    && make install

FROM postgres:15

COPY --from=build /usr/local/lib/libbech32.so* /usr/local/lib/
COPY --from=build /usr/lib/postgresql/15/lib/pg_bech32.so /usr/lib/postgresql/15/lib/
COPY --from=build /usr/share/postgresql/15/extension/pg_bech32* /usr/share/postgresql/15/extension/
RUN ldconfig
