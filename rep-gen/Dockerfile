FROM rust:1-slim-bookworm AS build
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
COPY static ./static
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 --home /nonexistent --shell /usr/sbin/nologin appuser
WORKDIR /app
COPY --from=build /app/target/release/rep-gen /usr/local/bin/rep-gen
USER appuser
EXPOSE 3000
ENV REP_GEN_BIND=0.0.0.0:3000
CMD ["/usr/local/bin/rep-gen"]
