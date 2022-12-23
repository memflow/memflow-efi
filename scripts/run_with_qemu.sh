#!/bin/bash
# start qemu with an exposed serial console on port 8080
qemu-system-x86_64 -enable-kvm \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_CODE.fd \
    -drive if=pflash,format=raw,readonly=on,file=/usr/share/edk2/x64/OVMF_VARS.fd \
    -drive format=raw,file=efi.raw \
    -serial tcp::8080,server &

# wait for qemu startup
while :
do
	sleep 1
    socat -,raw,echo=0,crnl tcp4:localhost:8080
done
