#!/bin/sh
if [ -z "$1" ]; then
    profile="debug"
else
    profile="release"
fi;

mkdir -p _efi/EFI/Boot/
cp Shell.efi _efi/EFI/Boot/Bootx64.efi
cp ../target/x86_64-unknown-uefi/$profile/memflow-efi-service.efi _efi/memflow.efi
mkimg -i _efi -o efi.raw
