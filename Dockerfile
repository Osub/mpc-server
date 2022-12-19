# syntax=docker/dockerfile:1
FROM rust:1.63 as build
WORKDIR /go/src/github.com/avalido/mpc-server/
COPY . .
RUN curl -LO https://github.com/protocolbuffers/protobuf/releases/download/v3.15.8/protoc-3.15.8-linux-x86_64.zip
RUN unzip protoc-3.15.8-linux-x86_64.zip -d $HOME/.local
ENV PATH="$PATH:$HOME/.local/bin"
RUN cargo build --release
RUN cd ./messenger && cargo build --release

FROM ubuntu:20.04
WORKDIR /app/
RUN apt-get update
RUN apt-get install -y curl
COPY --from=build /go/src/github.com/avalido/mpc-server/target/release/mpc-server ./
COPY --from=build /go/src/github.com/avalido/mpc-server/messenger/target/release/messenger ./
CMD ["./mpc-server"]
