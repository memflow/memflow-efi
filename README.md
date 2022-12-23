# memflow-efi

Memflow connector that utilizes an efi service (much like in the efi-memory project) to access physical memory.

## Creating the Image

Install prerequisites:
- virt-make-fs (`pacman -S guestfs-tools`)
- qemu-img (`pacman -S qemu`)
