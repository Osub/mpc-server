# syntax=docker/dockerfile:1
FROM rust:1.63-alpine3.16 as build
WORKDIR /go/src/github.com/avalido/mpc-server/
RUN apk add --no-cache musl-dev openssl-dev gmp-dev
RUN rustup target add x86_64-unknown-linux-musl
COPY . .
RUN cargo build --verbose --release --target x86_64-unknown-linux-musl
RUN cd ./messenger && RUSTFLAGS="-C target-feature=-crt-static" cargo build --verbose --release --target x86_64-unknown-linux-musl

FROM alpine:3.16
WORKDIR /app/
RUN apk add --no-cache bash
COPY --from=build /go/src/github.com/avalido/mpc-server/target/x86_64-unknown-linux-musl/release/mpc-server ./
COPY --from=build /go/src/github.com/avalido/mpc-server/messenger/target/x86_64-unknown-linux-musl/release/messenger ./
CMD ["./mpc-server"]