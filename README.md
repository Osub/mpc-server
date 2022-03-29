## MPC Server

Beside the main project, there are two auxiliary projects.
1. messenger

   It serves as the message broker for all the MPC players to broadcast their messages. This is used for the time being and it will be replaced by a P2P network eventually.
2. secp256k1-id

   It is a tool that creates a private-public key pairs that will be used to identify the player in the network.

### How to run the demo?
1. Build the 3 projects
```
cd messenger
cargo build
cd ../secp256k1-id
cargo build
cd ..
cargo build
```
2. Run the demo script
```
./scripts/demo.sh
```