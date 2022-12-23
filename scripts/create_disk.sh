#!/bin/sh
if [ -z "$1" ]; then
    profile="debug"
else
    profile="release"
fi;

mkdir -p _efi/EFI/Boot/
cp ../target/x86_64-unknown-uefi/$profile/memflow-efi-service.efi _efi/EFI/Boot/Bootx64.efi
virt-make-fs --type=vfat --size=24M _efi efi.raw
qemu-img convert -O vmdk efi.raw efi.vmdk