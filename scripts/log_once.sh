#!/bin/bash

if [ -z "$1" ]; then
    profile="release"
else
    profile="debug"
fi;

file="../target/x86_64-unknown-uefi/$profile/memflow-efi-service.efi"

sym=$(objdump -t "$file" | grep MEM_LOGGER)

#echo $sym

logger_addr=$(echo "$sym" | grep -Po '0x0000([0-9]|[A-F])*')
section=$(echo "$sym" | grep -Po 'sec *[0-9]+' | grep -Po '[0-9]*')
base=0x$(objdump -h "$file" | grep "  $(($section - 1)) " | awk '{print $4}')

if [ $(($base)) -gt $((0x100000000)) ]; then
	base=$(($base - 0x40000000))
fi

#echo "$logger_addr | $section"
#echo "$base"

skip_pages=$((("$logger_addr" + "$base") / 0x1000))

#echo "$skip_pages"

if [ -f /tmp/mem ]; then
	dd if=/tmp/mem bs=4096 skip=$skip_pages count=1 2>/dev/null
fi
