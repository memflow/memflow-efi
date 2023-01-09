#!/bin/bash

if [ -z "$OVMF_PREFIX" ]; then
    OVMF_PREFIX="/usr/share/edk2/x64"
fi

if [ -z "${QEMU_ARGS+x}" ]; then
    QEMU_ARGS="-enable-kvm"
fi

# start qemu with an exposed serial console on port 9080
rm /tmp/mem
qemu-system-x86_64 \
    -drive if=pflash,format=raw,readonly=on,file="${OVMF_PREFIX}/OVMF_CODE.fd" \
    -drive if=pflash,format=raw,readonly=on,file="${OVMF_PREFIX}/OVMF_VARS.fd" \
    -drive format=raw,file=efi.raw \
    $QEMU_ARGS \
	-object memory-backend-file,id=pc.ram,size=8G,mem-path=/tmp/mem,prealloc=on,share=on \
	-machine memory-backend=pc.ram \
    -m 8G \
	-monitor stdio \
	-serial null \
	-serial null \
	-serial null
    #-serial tcp::9080,server

# wait for qemu startup
#while :
#do
#	sleep 1
#    socat -,raw,echo=0,crnl tcp4:localhost:9080
#done
