# 3DS Dump Notes

## Local Files

The local `3ds_dump` directory contains a New3DS NAND image and GodMode9 essential backup:

- `3ds_dump/250102_QJF11332355_sysnand_00.bin`: `file` identifies this as a `Nintendo 3DS eMMC dump (New3DS)`, size about 1.3 GiB.
- `3ds_dump/250102_QJF11332355_sysnand_00.bin.sha`: raw 32-byte SHA-256 digest, not a text `sha256sum` file. The digest matches the NAND image: `1c8e50a4d3fe862e0dfed0efc7fec8a14a613504f68e105813bb15c4b342aca6`.
- `3ds_dump/QJF11332355_essential_00.exefs`: GodMode9 essential backup containing `nand_hdr`, `secinfo`, `movable`, `frndseed`, `nand_cid`, `otp`, `hwcal0`, and `hwcal1`.

Treat `QJF11332355_essential_00.exefs` and files extracted from it as console-unique secret material. Do not paste OTP, movable, friend seed, NAND CID, SecureInfo, or hardware-calibration bytes into notes or commits.

The 3DS Hacks Guide says `essential.exefs` can be used to recover data in the event of hardware failure. That is true for console/data recovery workflows because it contains console-unique material, but it is not the same as having every key input needed by PC-side NAND tools. `essential.exefs` does not include `boot9.bin`/`boot9_prot.bin`; those are dumped separately by boot9strap/GodMode9 key combos and provide the retail key material that tools such as `ninfs` need to derive NAND keys.

## Extracted Outputs

Generated outputs were written under ignored `3ds_dump` paths:

- `3ds_dump/extracted/essential_exefs/`: extracted GodMode9 essential ExeFS entries.
- `3ds_dump/derived/essential_exefs_manifest.txt`: `ctrtool -t exefs -v` manifest for the essential backup.
- `3ds_dump/derived/nand_partition_table.tsv`: parsed NCSD partition table.
- `3ds_dump/derived/nand_partition_table.json`: same partition table as JSON.
- `3ds_dump/extracted/nand_partitions/ncsd_header_from_nand.bin`: first 0x200 bytes from the NAND image.
- `3ds_dump/extracted/nand_partitions/01_agb_save.encrypted.bin`: encrypted AGB save partition.
- `3ds_dump/extracted/nand_partitions/02_firm0.encrypted.bin`: encrypted FIRM0 partition.
- `3ds_dump/extracted/nand_partitions/03_firm1.encrypted.bin`: encrypted FIRM1 partition.
- `3ds_dump/derived/extracted_partition_hashes.tsv`: SHA-256 hashes for the extracted partition chunks.

The large CTRNAND partition was not copied out separately. It spans `0x0b930000..0x4d800000` in the NAND image and should be mounted/decrypted in place with a 3DS-aware tool.

## NCSD Partition Map

Parsed from `nand_hdr.bin`:

| Index | Name | FS Type | Crypt Type | Start | Size | End |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | `twl_nand_region` | `0x01` | `0x01` | `0x0` | `0xb100000` | `0xb100000` |
| 1 | `agb_save` | `0x04` | `0x02` | `0xb100000` | `0x30000` | `0xb130000` |
| 2 | `firm0` | `0x03` | `0x02` | `0xb130000` | `0x400000` | `0xb530000` |
| 3 | `firm1` | `0x03` | `0x02` | `0xb530000` | `0x400000` | `0xb930000` |
| 4 | `ctrnand` | `0x01` | `0x03` | `0xb930000` | `0x41ed0000` | `0x4d800000` |

This matches the 3dbrew New3DS NAND layout: TWL region first, then AGB save, two FIRM partitions, then New3DS CTRNAND.

## Tool Results

- `ctrtool -t exefs -v 3ds_dump/QJF11332355_essential_00.exefs` successfully parses the GodMode9 essential backup.
- `ctrtool -t exefs --exefsdir=3ds_dump/extracted/essential_exefs 3ds_dump/QJF11332355_essential_00.exefs` extracts the essential entries.
- `ctrtool -t ncsd -v 3ds_dump/250102_QJF11332355_sysnand_00.bin` fails because the NAND image is not a CCI/gamecard NCSD image; parsing the 0x200-byte NAND NCSD header manually works.
- `ctrtool -t firm -v 3ds_dump/extracted/nand_partitions/02_firm0.encrypted.bin` fails with invalid magic because the FIRM partition is still NAND-encrypted.
- `binwalk` and `strings` on the encrypted NAND/FIRM chunks do not produce useful module strings.

## Next Extraction Step

The current blocker is NAND decryption, not partition discovery. The flake now includes general support tools (`ctrtool`, `fuse`, `fuse3`, `mtools`, `sleuthkit`, Python crypto/FUSE libraries, `pip`) and a flake-local `ninfs` v1.7b2 package because nixpkgs did not contain `ninfs`/`fuse-3ds` or `3dstool` directly.

`ninfs` also requires the 3DS ARM9 boot ROM for 3DS mounts. It checks `--boot9`, `BOOT9_PATH`, `~/.3ds/boot9.bin`, `~/.3ds/boot9_prot.bin`, `~/3ds/boot9.bin`, and `~/3ds/boot9_prot.bin`. Either `boot9.bin` or `boot9_prot.bin` can be used. The current dump set includes `otp.bin` and `nand_cid.bin`, but no Boot9 file was found or generated in this pass. Running `mount_nandctr --otp 3ds_dump/extracted/essential_exefs/otp.bin --cid 3ds_dump/extracted/essential_exefs/nand_cid.bin -r -f 3ds_dump/250102_QJF11332355_sysnand_00.bin 3ds_dump/mount` currently stops at `Bootrom could not be found`.

The practical next step is to run `ninfs` or equivalent 3DS NAND tooling against:

- NAND image: `3ds_dump/250102_QJF11332355_sysnand_00.bin`
- Essential backup: `3ds_dump/QJF11332355_essential_00.exefs`
- Extracted essential directory: `3ds_dump/extracted/essential_exefs/`
- Boot9 path when available: pass `--boot9 /path/to/boot9.bin` or set `BOOT9_PATH=/path/to/boot9.bin`

Once CTRNAND is mounted/decrypted, collect the CECD/NWM targets for StreetPass RE:

- `nand/title/00040130/...`: system modules, especially `cecd` and `nwm`.
- `nand/data/<ID0>/sysdata/00010026`: CECD StreetPass system save.
- `nand/title/00040010/...`: StreetPass Mii Plaza and local communication system applications, if present for the region.

## References

- 3dbrew Flash Filesystem: https://www.3dbrew.org/wiki/Flash_Filesystem
- 3dbrew NCSD: https://www.3dbrew.org/wiki/NCSD
- 3dbrew Title list: https://www.3dbrew.org/wiki/Title_list
