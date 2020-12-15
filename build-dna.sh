#!/bin/bash

pushd zomes/rss_pub
RUST_BACKTRACE=1 CARGO_TARGET_DIR=target cargo build \
  --release --target wasm32-unknown-unknown
popd

pushd dna
dna-util -c rss_pub.dna.workdir
popd
