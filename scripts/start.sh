#!/usr/bin/env bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
WORK_DIR=$SCRIPT_DIR/../target/debug
MESSENGER_DIR=$SCRIPT_DIR/../messenger/target/debug

pkill -f mpc-server
pkill -f messenger

rm -rf p*.*

sleep 1
echo -n "f6826fc16547130848ea32196c95457b4698feded1a8f109eb224ddcd27d66af7d0f88b44d2765a4a567a8999a4410852510deacdcb87e0c7cfe23fa1d0090a8" > ${WORK_DIR}/p1.s
echo -n "9f099ed7a7615ce2d7de100a8feaf39adfa904146ee523f6dacaad6a3f69b4e9d10f2b01c13dc31b1265f9042d7437f63ad81c5113dec541bb799e49dd21c571" > ${WORK_DIR}/p2.s
echo -n "e3858ec05c7762d79de30428a2561a520123b1e2b16687ae57ba0ba550e07ffac49861bab15fe62678435c8ae2a57522752a7b8af68d7591ca0e7b44c5f64a4d" > ${WORK_DIR}/p3.s

RUST_BACKTRACE=full ${MESSENGER_DIR}/messenger --address 0.0.0.0 --port 8000 > ${MESSENGER_DIR}/messenger.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p1.s --password RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL --address 0.0.0.0 --port 8001 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p1.db > ${WORK_DIR}/p1.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p2.s --password RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL --address 0.0.0.0 --port 8002 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p2.db > ${WORK_DIR}/p2.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p3.s --password RBuCJbmWY1Mtcl5LoMRqkQQpT5GJmCEvbuRR7ewCPDATBzFtm9a6jhIovftgddmL --address 0.0.0.0 --port 8003 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p3.db > ${WORK_DIR}/p3.log 2>&1 &
