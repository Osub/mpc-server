#!/usr/bin/env bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
export RUST_LOG="warn,mpc_server=debug"

NUM_PARTIES=3
PUBKEYS=("03c20e0c088bb20027a77b1d23ad75058df5349c7a2bfafff7516c44c6f69aa66d" "03d0639e479fa1ca8ee13fd966c216e662408ff00349068bdc9c6966c4ea10fe3e" "0373ee5cd601a19cd9bb95fe7be8b1566b73c51d3e7e375359c129b1d77bb4b3e6")
SECKEYS=("f6826fc16547130848ea32196c95457b4698feded1a8f109eb224ddcd27d66af7d0f88b44d2765a4a567a8999a4410852510deacdcb87e0c7cfe23fa1d0090a8" "9f099ed7a7615ce2d7de100a8feaf39adfa904146ee523f6dacaad6a3f69b4e9d10f2b01c13dc31b1265f9042d7437f63ad81c5113dec541bb799e49dd21c571" "e3858ec05c7762d79de30428a2561a520123b1e2b16687ae57ba0ba550e07ffac49861bab15fe62678435c8ae2a57522752a7b8af68d7591ca0e7b44c5f64a4d")
#PLAINSECKEYS=("59d1c6956f08477262c9e827239457584299cf583027a27c1d472087e8c35f21" "6c326909bee727d5fc434e2c75a3e0126df2ec4f49ad02cdd6209cf19f91da33" "5431ed99fbcc291f2ed8906d7d46fdf45afbb1b95da65fecd4707d16a6b3301b")

gen_keypair() {
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  echo "Preparing key-pair for party ${IND}"
  /bin/echo -n "${PUBKEYS[$((IND-1))]}" > $DIR/pub.key
  /bin/echo -n "${SECKEYS[$((IND-1))]}" > $DIR/pri.key
}

start_mpc_server(){
  IND=$1
  DIR=tmp/party${IND}
  mkdir -p $DIR
  PORT=800${IND}
  echo "Starting MPC Server ${i}"
  RUST_BACKTRACE=full $SCRIPT_DIR/../target/debug/mpc-server -s ${DIR}/pri.key \
  --password RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL \
  --port ${PORT} -m http://127.0.0.1:8000  --db-path ${DIR}/db > ${DIR}/log.txt 2>&1 &
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
sleep 5

echo "5. Checking Keygen result"
pubkey=$(curl --silent -X POST http://127.0.0.1:8001/result/001 | grep -o -w -E '[[:alnum:]]{66}')
echo "Generated pubkey ${pubkey}"


echo "6. Run Sign"

PAYLOAD=$(cat <<-END
  { "request_id": "sign-001", "message": "7F83B1657FF1FC53B92DC18148A1D65DFC2D4B1FA3D677284ADDD200126D9069", "participant_public_keys": ["${PK2}", "${PK3}"], "public_key": "${pubkey}"}
END
)
curl --silent -X POST http://127.0.0.1:8002/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"
curl --silent -X POST http://127.0.0.1:8003/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"

echo -e "\nWaiting for Sign to complete"
sleep 5

echo "7. Checking Sign result"
curl -X POST http://127.0.0.1:8002/result/sign-001

echo "\n8. Checking metrics"
curl -X GET http://127.0.0.1:8002/metrics
