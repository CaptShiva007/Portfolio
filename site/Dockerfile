# ---- build ----
FROM rust:1-slim-bookworm AS build
WORKDIR /app
# cache dependencies first
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs && cargo build --release && rm -rf src
COPY src src
COPY assets assets
RUN touch src/main.rs && cargo build --release

# ---- run ----
FROM debian:bookworm-slim
RUN useradd -r -u 10001 app
COPY --from=build /app/target/release/prism-site /usr/local/bin/prism-site
COPY --from=build /app/assets ./assets
USER app
ENV BIND=0.0.0.0:3000 CONTENT_DIR=/content
EXPOSE 3000
CMD ["prism-site"]
