#!/bin/bash
cd ../memflow-efi-service
cargo +nightly build --release --target x86_64-unknown-uefi

cd ../scripts
./create_disk.sh release
