#!/usr/bin/env bash

REQID=$1

if [ -z $REQID ]
then
    echo "Please supply request id."
    exit 1
fi

echo "Run Keygen"

PK1='03c20e0c088bb20027a77b1d23ad75058df5349c7a2bfafff7516c44c6f69aa66d'
PK2='03d0639e479fa1ca8ee13fd966c216e662408ff00349068bdc9c6966c4ea10fe3e'
PK3='0373ee5cd601a19cd9bb95fe7be8b1566b73c51d3e7e375359c129b1d77bb4b3e6'
PAYLOAD=$(cat <<-END
  {"request_id": "keygen-001", "public_keys": ["${PK1}", "${PK2}", "${PK3}"], "t": 1}
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

echo "Checking Keygen result"
pubkey=$(curl --silent -X POST http://127.0.0.1:8001/result/keygen-001 | grep -o -w -E '[[:alnum:]]{66}')
echo "Generated pubkey ${pubkey}"


echo "Run Sign"

PAYLOAD=$(cat <<-END
  { "request_id": "${REQID}", "message": "7F83B1657FF1FC53B92DC18148A1D65DFC2D4B1FA3D677284ADDD200126D9069", "participant_public_keys": ["${PK2}", "${PK3}"], "public_key": "${pubkey}"}
END
)
curl --silent -X POST http://127.0.0.1:8002/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"
sleep 5
curl --silent -X POST http://127.0.0.1:8003/sign \
     -H "Content-Type: application/json" \
     -d "${PAYLOAD}"

echo -e "\nWaiting for Sign to complete"
sleep 5

echo "Checking Sign result"
curl -X POST http://127.0.0.1:8002/result/$REQID
