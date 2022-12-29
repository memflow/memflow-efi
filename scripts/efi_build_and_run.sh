#!/bin/bash
./efi_build.sh "$1"
./run_with_qemu.sh "$1"
