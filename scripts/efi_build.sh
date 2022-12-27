#!/bin/bash
cd ../memflow-efi-service
cargo build --release

cd ../scripts
./create_disk.sh release
