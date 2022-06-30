#!/usr/bin/env bash

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
WORK_DIR=$SCRIPT_DIR/../target/debug
MESSENGER_DIR=$SCRIPT_DIR/../messenger/target/debug

pkill -f mpc-server
pkill -f messenger

rm -rf p*.*

sleep 1
echo -n "59d1c6956f08477262c9e827239457584299cf583027a27c1d472087e8c35f21" > ${WORK_DIR}/p1.s
echo -n "6c326909bee727d5fc434e2c75a3e0126df2ec4f49ad02cdd6209cf19f91da33" > ${WORK_DIR}/p2.s
echo -n "5431ed99fbcc291f2ed8906d7d46fdf45afbb1b95da65fecd4707d16a6b3301b" > ${WORK_DIR}/p3.s

RUST_BACKTRACE=full ${MESSENGER_DIR}/messenger --address 0.0.0.0 --port 8000 > ${MESSENGER_DIR}/messenger.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p1.s --address 0.0.0.0 --port 8001 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p1.db > ${WORK_DIR}/p1.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p2.s --address 0.0.0.0 --port 8002 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p2.db > ${WORK_DIR}/p2.log 2>&1 &
RUST_BACKTRACE=full ${WORK_DIR}/mpc-server -s ${WORK_DIR}/p3.s --address 0.0.0.0 --port 8003 -m http://127.0.0.1:8000  --db-path ${WORK_DIR}/p3.db > ${WORK_DIR}/p3.log 2>&1 &
