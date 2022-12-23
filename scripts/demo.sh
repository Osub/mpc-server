#!/usr/bin/env bash
realpath() {
    [[ $1 = /* ]] && echo "$1" || echo "$PWD/${1#./}"
}
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
PROTO_DIR=${SCRIPT_DIR}/../proto
export RUST_LOG="debug"

USE_REDIS=$1
MQ_CONFIG=$([[ ! -z "$USE_REDIS" ]] && echo "-r redis://localhost/" || echo "-m http://127.0.0.1:8000")

echo "Using message queue $MQ_CONFIG"

NUM_PARTIES=3
PUBKEYS=(
'0373ee5cd601a19cd9bb95fe7be8b1566b73c51d3e7e375359c129b1d77bb4b3e6'
'03c20e0c088bb20027a77b1d23ad75058df5349c7a2bfafff7516c44c6f69aa66d'
'03d0639e479fa1ca8ee13fd966c216e662408ff00349068bdc9c6966c4ea10fe3e'
)
SECKEYS=(
'{"address":"3600323b486f115ce127758ed84f26977628eeaa","crypto":{"cipher":"aes-128-ctr","ciphertext":"11ea4ed8f5682ba1ebc6369b484f42ad31ca1d250ecbf123a489e1589590d000","cipherparams":{"iv":"274338bdcb1f715198d7a3afcd88e84a"},"kdf":"scrypt","kdfparams":{"dklen":32,"n":262144,"p":1,"r":8,"salt":"fdc6be82757e600e97bfc61402ed41a11afcd2ef36a8d112038459ee7af0ec48"},"mac":"775ce29e39855ea5fe53c140c0c7929a612a0437fd3aa2311da862680f670948"},"id":"66604f01-65d3-4a42-bb0e-fce3cbd8d03f","version":3}'
'{"address":"3051ba2d313840932b7091d2e8684672496e9a4b","crypto":{"cipher":"aes-128-ctr","ciphertext":"71ca430ab0785a421421f9107e842b90423df3e1682aeeed058ccab575a0fef9","cipherparams":{"iv":"295c3ddc4c05f8bf777c6940bede075d"},"kdf":"scrypt","kdfparams":{"dklen":32,"n":262144,"p":1,"r":8,"salt":"349e267e65b294edc7fd34b59b42a255a417e01dacf760fddef16d89c0dbaabd"},"mac":"9b5ba31d30bde66a1a5e774f8c73699cefc1ad4fee1f16f1c4193834b4ce5f73"},"id":"2a14b094-c7a9-41aa-84bd-c7ad228fc78b","version":3}'
'{"address":"7ac8e2083e3503be631a0557b3f2a8543eaadd90","crypto":{"cipher":"aes-128-ctr","ciphertext":"2275391cc83a56fc97fbdd2954e647ee974f4812b188a5499dc80c58f9ccb2ac","cipherparams":{"iv":"8ebb1ea980880135326e65ef422dc142"},"kdf":"scrypt","kdfparams":{"dklen":32,"n":262144,"p":1,"r":8,"salt":"4f4c6e84a4298402e33ae1c7443f30ec20a34faa92d6411ed870e640e1b12b5d"},"mac":"852431422f726097223adaa3ded2497dfc8268c7ba56b81f7bf2a357f5e7e4db"},"id":"8ad90eb0-663f-46c5-9ea5-b10f2e0aa759","version":3}'
)

gen_keypair() {
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  echo "Preparing key-pair for party ${IND}"
  /bin/echo -n "RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL" > $DIR/password
  /bin/echo -n "${SECKEYS[$((IND-1))]}" > $DIR/key
}

start_mpc_server(){
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  PORT=800${IND}
  echo "Starting MPC Server ${i}"
  RUST_BACKTRACE=full $SCRIPT_DIR/../target/debug/mpc-server \
  --keystore-path ${DIR}/key \
  --password-path ${DIR}/password \
  --port ${PORT} $MQ_CONFIG  --db-path ${DIR}/db > ${DIR}/log.txt 2>&1 &
}

# 0. Clean up
for p in $( ps ax | grep mpc-server | awk '{print $1;}' ); do
  kill -9 $p
done

pkill -f messenger
rm -rf tmp

# 1. Generate key-pairs
echo "1. Generating identity keys"
for i in $(seq $NUM_PARTIES);
  do gen_keypair $i;
done

# 2. Start messenger
echo "2. Starting messenger"
mkdir -p $SCRIPT_DIR/../tmp/messenger
$SCRIPT_DIR/../messenger/target/debug/messenger > $SCRIPT_DIR/../tmp/messenger/log.txt 2>&1 &

# 3. Start MPC Servers
echo "3. Starting MPC Servers"
for i in $(seq $NUM_PARTIES);
  do start_mpc_server $i;
done

# 4. Run Keygen
echo "Waiting for server to start up"
sleep 5

echo "4. Run Keygen"
PK1=${PUBKEYS[0]}
PK2=${PUBKEYS[1]}
PK3=${PUBKEYS[2]}
PAYLOAD=$(cat <<-END
  {"request_id": "001", "participant_public_keys": ["${PK1}", "${PK2}", "${PK3}"], "threshold": 1}
END
)

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8001 mpc.Mpc/Keygen

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8002 mpc.Mpc/Keygen

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8003 mpc.Mpc/Keygen

echo -e "\nWaiting for Keygen to complete"
sleep 50

echo "5. Checking Keygen result"
PAYLOAD=$(cat <<-END
  {"request_id": "001"}
END
)
pubkey=$(grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8001 mpc.Mpc/CheckResult | grep -o -w -E '[[:alnum:]]{66}')
echo "Generated pubkey ${pubkey}"


echo "6. Run Sign"

PAYLOAD=$(cat <<-END
  { "request_id": "sign-001", "hash": "7F83B1657FF1FC53B92DC18148A1D65DFC2D4B1FA3D677284ADDD200126D9069", "participant_public_keys": ["${PK2}", "${PK3}"], "public_key": "${pubkey}"}
END
)

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8002 mpc.Mpc/Sign

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8003 mpc.Mpc/Sign

echo -e "\nWaiting for Sign to complete"
sleep 5

echo "7. Checking Sign result"

PAYLOAD=$(cat <<-END
  {"request_id": "sign-001"}
END
)

grpcurl -plaintext -import-path ${PROTO_DIR} -proto mpc.proto -d "${PAYLOAD}" 127.0.0.1:8002 mpc.Mpc/CheckResult

echo "\n8. Checking metrics"
curl -X GET http://127.0.0.1:8102/metrics
