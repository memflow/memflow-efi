#!/bin/sh
if [ -z "$1" ]; then
    profile="release"
else
    profile="debug"
fi;

rm -rf _efi
mkdir -p _efi/
cp ../target/x86_64-unknown-uefi/$profile/memflow-efi-service.efi _efi/memflow.efi
cp -r efi_include/* _efi/
mkimg -i _efi -p gpt -o efi.raw
