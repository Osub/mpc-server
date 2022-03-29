#!/usr/bin/env bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

NUM_PARTIES=3

gen_keypair() {
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  echo "Generating key-pair for party ${IND}"
  $SCRIPT_DIR/../secp256k1-id/target/debug/secp256k1-id -p $DIR/pub.key -s $DIR/pri.key
}

start_mpc_server(){
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  PORT=800${IND}
  echo "Starting MPC Server ${i}"
  RUST_BACKTRACE=full $SCRIPT_DIR/../target/debug/mpc-server -s ${DIR}/pri.key --port ${PORT} -m http://127.0.0.1:8000  --db-path ${DIR}/db > ${DIR}/log.txt 2>&1 &
}

# 0. Clean up
pkill -f mpc-server
pkill -f messenger
rm -rf tmp

# 1. Generate key-pairs
echo "1. Generating identity keys"
for i in $(seq $NUM_PARTIES);
  do gen_keypair $i;
done

# 2. Start messenger
echo "2. Starting messenger"
$SCRIPT_DIR/../messenger/target/debug/messenger &>/dev/null &

# 3. Start MPC Servers
echo "3. Starting MPC Servers"
for i in $(seq $NUM_PARTIES);
  do start_mpc_server $i;
done

# 4. Run Keygen
echo "Waiting for server to start up"
sleep 5s

echo "4. Run Keygen"
PK1=$(cat tmp/party1/pub.key)
PK2=$(cat tmp/party2/pub.key)
PK3=$(cat tmp/party3/pub.key)
PAYLOAD=$(cat <<-END
  {"request_id": "001", "public_keys": ["${PK1}", "${PK2}", "${PK3}"], "t": 1}
END
)

curl --silent -X POST http://127.0.0.1:8001/keygen \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"
curl --silent -X POST http://127.0.0.1:8002/keygen \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"
curl --silent -X POST http://127.0.0.1:8003/keygen \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"

echo -e "\nWaiting for Keygen to complete"
sleep 5s

echo "5. Checking Keygen result"
pubkey=$(curl --silent -X POST http://127.0.0.1:8001/result/001 | grep -o -w -E '[[:alnum:]]{66}')
echo "Generated pubkey ${pubkey}"


echo "6. Run Sign"

PAYLOAD=$(cat <<-END
  { "request_id": "sign-001", "message": "Hello", "participant_public_keys": ["${PK1}", "${PK2}"], "public_key": "${pubkey}"}
END
)
curl --silent -X POST http://127.0.0.1:8001/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"
curl --silent -X POST http://127.0.0.1:8002/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"

echo -e "\nWaiting for Sign to complete"
sleep 5s

echo "7. Checking Sign result"
curl -X POST http://127.0.0.1:8001/result/sign-001
