FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM alpine:3
RUN apk add --no-cache ca-certificates && adduser -D -u 65532 mcp
COPY --from=build /src/target/release/pocket-id-mcp /usr/local/bin/pocket-id-mcp
USER mcp
# stdio by default; set POCKET_ID_MCP_TRANSPORT=http (and bind 0.0.0.0:8756) for HTTP mode
EXPOSE 8756
ENTRYPOINT ["/usr/local/bin/pocket-id-mcp"]
