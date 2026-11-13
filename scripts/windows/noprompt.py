#!/usr/bin/env python3
# noprompt.py <iso>: makes a Windows ISO boot without "Press any key to boot from CD or DVD", for
# windows-vm.sh, which runs it on its own copy of the ISO (an APFS clone, so only what changes
# takes room). Nobody presses a key in a VM that installs itself, and when none is pressed the
# firmware boots the next thing, an empty disk.
#
# The firmware boots the EFI image the ISO's El Torito boot catalog points to, a copy of
# efi/microsoft/boot/efisys.bin. Microsoft ships efisys_noprompt.bin beside it, the same image
# without the prompt and of the same size, so this writes that file's bytes over the image the
# catalog points to, after checking the image is efisys.bin's. The files are found through the
# ISO's UDF file system (its ISO 9660 one lists only a readme).
import struct
import sys

SECTOR = 2048
iso = open(sys.argv[1], "r+b")


def sectors(lba, count=1):
    iso.seek(lba * SECTOR)
    return iso.read(count * SECTOR)


# El Torito: the boot record volume descriptor names the catalog, whose entries say where each
# boot image is: those after a section header for platform 0xEF are EFI images, and are tried
# first. An ISO made with mkisofs (UUP dump's convert.sh) says every image is for x86, a BIOS
# one first and the EFI one after it, which UEFI firmware boots all the same; so the rest are
# tried too, each checked to be efisys.bin before anything is written.
def boot_image_lbas():
    for n in range(16, 64):
        d = sectors(n)
        if d[0] == 0 and d[7:30] == b"EL TORITO SPECIFICATION":
            catalog = sectors(struct.unpack("<I", d[71:75])[0])
            break
        if d[0] == 255:
            sys.exit("no El Torito boot record in the ISO")
    platform = catalog[1]  # the validation entry's, for the default entry after it
    efi, other = [], []
    for i in range(32, SECTOR, 32):
        e = catalog[i : i + 32]
        if e[0] in (0x90, 0x91):
            platform = e[1]
        elif e[0] == 0x88:
            (efi if platform == 0xEF else other).append(struct.unpack("<I", e[8:12])[0])
    return efi + other


# UDF: anchor at sector 256 → volume descriptors (partition start, file set descriptor) → root
# directory → efi/microsoft/boot.
avdp = sectors(256)
length, location = struct.unpack("<II", avdp[16:24])
vds = sectors(location, length // SECTOR)
part_start = fsd_lbn = None
for i in range(0, len(vds), SECTOR):
    d = vds[i : i + SECTOR]
    tag = struct.unpack("<H", d[0:2])[0]
    if tag == 5:
        part_start = struct.unpack("<I", d[188:192])[0]
    elif tag == 6:
        fsd_lbn = struct.unpack("<I", d[252:256])[0]
    elif tag == 8:
        break


def entry(lbn):
    """A file entry's length and extents (absolute sector, bytes), or its data if held inline."""
    fe = sectors(part_start + lbn)
    tag = struct.unpack("<H", fe[0:2])[0]
    head = {261: 176, 266: 216}[tag]  # File Entry, Extended File Entry
    size = struct.unpack("<Q", fe[56:64])[0]
    l_ea, l_ad = struct.unpack("<II", fe[head - 8 : head])
    start = head + l_ea
    kind = struct.unpack("<H", fe[34:36])[0] & 7
    if kind == 3:
        return size, None, fe[start : start + l_ad]
    step = {0: 8, 1: 16}[kind]  # short and long allocation descriptors
    extents = []
    for j in range(start, start + l_ad, step):
        n, pos = struct.unpack("<II", fe[j : j + 8])
        extents.append((part_start + pos, n & 0x3FFFFFFF))
    return size, extents, None


def read(lbn):
    size, extents, inline = entry(lbn)
    if inline is not None:
        return inline
    data = b"".join(sectors(pos, (n + SECTOR - 1) // SECTOR)[:n] for pos, n in extents)
    return data[:size]


def listdir(lbn):
    data, i, names = read(lbn), 0, {}
    while i < len(data):
        l_fi = data[i + 19]
        icb = struct.unpack("<I", data[i + 24 : i + 28])[0]
        l_iu = struct.unpack("<H", data[i + 36 : i + 38])[0]
        raw = data[i + 38 + l_iu : i + 38 + l_iu + l_fi]
        if l_fi:
            name = raw[1:].decode("latin1" if raw[0] == 8 else "utf-16-be")
            names[name.lower()] = icb
        i += (38 + l_iu + l_fi + 3) & ~3
    return names


root = struct.unpack("<I", sectors(part_start + fsd_lbn)[404:408])[0]
folder = listdir(root)
for part in ("efi", "microsoft", "boot"):
    folder = listdir(folder[part])
prompt = read(folder["efisys.bin"])
noprompt = read(folder["efisys_noprompt.bin"])
if len(prompt) != len(noprompt):
    sys.exit("efisys.bin and efisys_noprompt.bin differ in size")

for lba in boot_image_lbas():
    image = sectors(lba, (len(prompt) + SECTOR - 1) // SECTOR)[: len(prompt)]
    if image == noprompt:
        print("the ISO already boots without asking")
        break
    if image == prompt:
        iso.seek(lba * SECTOR)
        iso.write(noprompt)
        print(f"wrote efisys_noprompt.bin over the EFI boot image at sector {lba}")
        break
else:
    sys.exit("none of the ISO's boot images is efisys.bin; left as it is")
