# Runtime image only. CI builds the binary and the database and copies both in, so a deploy is an
# immutable dataset and nothing compiles on Fly.
#
#   cargo build --release -p ddo-api
#   cargo run --release -p ddo-etl -- build --source upstream/Output/DataFiles --out ddo.db
#   docker build -t ddo-api .
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY target/release/ddo-api /usr/local/bin/ddo-api
COPY ddo.db /app/ddo.db
ENV DDO_DB_PATH=/app/ddo.db PORT=8080 RUST_LOG=info,tower_http=info
EXPOSE 8080
USER nobody
CMD ["ddo-api"]
