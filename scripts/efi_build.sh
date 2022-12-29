#!/bin/bash
cd ../memflow-efi-service

if [ -z "$1" ] || [ "$1" = "release" ]; then
    profile="--release"
fi

cargo build $profile

echo "$1"
echo "$profile"

cd ../scripts
./create_disk.sh "$1"
