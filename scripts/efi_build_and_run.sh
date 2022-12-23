#!/bin/bash
cargo +nightly build --bin memflow-efi-service --release --target x86_64-unknown-uefi
./create_disk.sh release
./run_with_qemu.sh