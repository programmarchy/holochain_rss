#!/bin/bash

pushd app/zomes/rss
RUST_BACKTRACE=1 CARGO_TARGET_DIR=target cargo build \
  --release --target wasm32-unknown-unknown
popd

pushd app
dna-util -c rss.dna.workdir
popd
