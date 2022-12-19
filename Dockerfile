# syntax=docker/dockerfile:1
FROM rust:alpine3.16 as build
WORKDIR /go/src/github.com/avalido/mpc-server/
COPY . .
RUN apk update && apk add protoc
RUN cargo build --release
RUN cd ./messenger && cargo build --release

FROM ubuntu:20.04
WORKDIR /app/
RUN apt-get update
RUN apt-get install -y curl
COPY --from=build /go/src/github.com/avalido/mpc-server/target/release/mpc-server ./
COPY --from=build /go/src/github.com/avalido/mpc-server/messenger/target/release/messenger ./
CMD ["./mpc-server"]
