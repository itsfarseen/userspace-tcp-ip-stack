#!/bin/sh
if cargo build; then
    sudo setcap cap_net_admin=eip ./target/debug/networks-mini-project
    cargo run&
    pid=$!
    trap "pkill networks-mini-p" INT TERM
    sleep 0.5s
    sudo ip addr add 10.0.0.1 dev tun0
    sudo ip link set tun0 up
    sudo ip route add 10.0.0.0/24 dev tun0
    wait $pid
fi
