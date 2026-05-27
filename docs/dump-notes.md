# Dump Notes

## Boot And Plugin Configuration

`vitadump/ur0:/tai/boot_config.txt` is a useful map of early kernel modules. The network and radio-related load entries are:

- `os0:kd/net_ps.skprx`
- `os0:kd/gps.skprx`
- `os0:kd/bbmc.skprx`
- `os0:kd/wlanbt.skprx`
- `os0:kd/usb_ether_smsc.skprx`
- `os0:kd/usb_ether_rtl.skprx`
- `os0:kd/bt.skprx`
- `os0:kd/marlin_hci.skprx`

The local `os0:/kd` dump includes `wlanbt_robin_img_ax.skprx`, but not `wlanbt.skprx`, `net_ps.skprx`, `bt.skprx`, or the USB Ethernet drivers listed in the boot config. The HENkaku wiki says decrypted `os0:kd/bootimage.skprx` contains a list of ELFs whose library information paths are prefixed with `bootfs:`. Its module list identifies `SceNetPs` as `bootfs:net_ps.skprx` and `SceWlanBt` as `bootfs:wlanbt.skprx` on firmware 3.60, so these are likely embedded in bootimage/bootfs rather than stored as normal `os0:/kd` files.

`vitadump/ur0:/tai/config.txt` shows a HENkaku/taiHEN setup with common kernel plugins:

- `0syscall6.skprx`
- `nonpdrm.skprx`
- `repatch.skprx`
- `PSVshell.skprx`
- `kubridge.skprx`
- `NoPowerLimits.skprx`

The same config also enables user plugins for Shell/settings version spoofing and quality-of-life UI changes.

## SELF Containers

Several original dumped binaries start with the Vita `SCE\0` header and are not directly readable by normal desktop tools:

- `vitadump/os0:/kd/wlanbt_robin_img_ax.skprx`
- `vitadump/os0:/kd/bootimage.skprx`
- `vitadump/vs0:/sys/external/libnet.suprx`
- `vitadump/vs0:/sys/external/libnetctl.suprx`
- `vitadump/vs0:/sys/external/pspnet_adhoc.suprx`
- `vitadump/vs0:/sys/external/adhoc_matching.suprx`

Plain `strings` produced no useful WLAN/API names from the original SELF containers. FAGDec output under `vitadump/ux0:/FAGDec` is much more useful:

- `kd/wlanbt_robin_img_ax.skprx.elf` is `ELF 32-bit LSB ARM, EABI5`, size 404024 bytes. The original SELF is 310941 bytes.
- `kd/bootimage.skprx.elf` is `ELF 32-bit LSB ARM, EABI5`, size 3190432 bytes. The original SELF is 1899181 bytes.
- `sys/external/libnet.suprx.elf`, `libnetctl.suprx.elf`, `adhoc_matching.suprx.elf`, and `pspnet_adhoc.suprx.elf` are also valid 32-bit ARM ELFs.
- `readelf -S` reports no sections for `wlanbt_robin_img_ax.skprx.elf`; treat it as firmware/load-image style ELF, not a normal symbolized application.

`bootimage.skprx.elf` contains 57 embedded ELF headers. The first chunk is `SceKernelBootimage` and contains path strings including `os0:kd/net_ps.skprx` and `os0:kd/wlanbt.skprx`. The target modules were extracted to:

- `vitadump/ux0:/FAGDec/kd/bootimage_modules/SceNetPs.bootfs.elf`, bootimage offset `0xd01ec`, size `0x3a530`.
- `vitadump/ux0:/FAGDec/kd/bootimage_modules/SceWlanBt.bootfs.elf`, bootimage offset `0x165b14`, size `0xe548`.

Both extracted modules are valid `ELF 32-bit LSB ARM, EABI5` files. They are stripped and have no section tables. `SceWlanBt.bootfs.elf` has entry point `0xc000`; `SceNetPs.bootfs.elf` has entry point `0x2ac60`.

Useful decrypted Robin strings found so far:

- `SceWlanBtRobinImageAx`
- `wlan`, `sdio`, `amp_sdio`, `bt_sdio`
- `MAC Tx`, `MAC Tx Notify`, `MAC Mgmt`
- `TxMgmt80211MsgQ`, `MacMgmt80211MsgQ`, `MacMgmtSMEMsgQ`
- `Marvell Micro AP`
- `$Id: w8787-Ax, RF878X, FP65, 14.65.9.p223, BT_SDIO $`

Useful `SceWlanBt.bootfs.elf` strings found so far:

- `SceWlanBtRobinWlanCommand`
- `SceWlanBtRobinWlan`
- `SceWlanBtRobinWlanSdioAggrRx`
- `SceWlanBtRobinWlanWakeConfig`
- `SceWlanBtRobinWlanContext`
- `SceWlanBtRobinWlanWorker`
- `SceNetDrvWlan`
- `[SceWlanBt]:  Command failed. (com=0x%04x, resp=0x%04x, result=%d)`
- `[SceWlanBt]: Skip unexpected Response. (resp=0x%04x)`
- `[SceWlanBt]: too large message size=%d`

Useful `SceNetPs.bootfs.elf` strings found so far:

- `SceNetPs`
- `SceNetPsForDriver`
- `SceNetPsForSyscalls`
- `SceNetKernelDevSend`
- `SceNetKernelDevRecv`
- `SceNetKernelPkt`
- `wlan0`
- `fake_3g_if`
- `bnet_socket_sanity_check`

## Ghidra Annotations

The following annotations were applied and saved in `SceWlanBt.bootfs.elf` in the `VitaWifiGhidra` project:

- `0x81001cd4` renamed to `HandleRobinWlanRxMessage`.
- `0x810020a8` renamed to `PollRobinWlanRxPackets`.
- `0x8100271c` renamed to `InitializeRobinWlanQueues`.
- `0x81002db4` renamed to `QueueRobinCommandAsync`.
- `0x81002f44` renamed to `SendRobinCommandSync`.
- `0x81006a6c` renamed to `BuildAndSendRobinScanCommand`.
- `0x81004b10` renamed to `BuildAndSendRobinJoinCommand`.
- `0x8100b570` renamed to `HandleWlanConnectRequest`.
- `0x8100940c` renamed to `InitializeWlanDriverContext`.
- `0x81003144` renamed to `HandleWlanRxInterrupt`.
- `0x810030c8` renamed to `TransmitQueuedRobinCommand`.
- `0x81003514` renamed to `SendRobinSimpleCommand28`.

Important command ids observed in this pass:

- `0x06`: scan command built by `BuildAndSendRobinScanCommand`.
- `0x12`: join/association command built by `BuildAndSendRobinJoinCommand`.
- `0x28`: simple control command with a 2-byte payload in `SendRobinSimpleCommand28`.
- Command responses are matched as `cmd | 0x8000` by `HandleRobinWlanRxMessage`.

Potentially useful custom-IE angles:

- `BuildAndSendRobinScanCommand` has an optional extra payload path up to `0x400` bytes.
- `BuildAndSendRobinJoinCommand` builds several 802.11 IE-like fields, including vendor-specific tag `0xdd`, but this is still firmware-controlled join/association behavior rather than arbitrary frame TX.

## Interesting Existing Files

- `vitadump/ur0:/tai/net_logging_mgr.skprx` is present and may be worth separate inspection if decrypted; the name suggests network logging hooks, but strings did not reveal useful details in the dumped SELF.
- `vitadump/vs0:/sys/external/libcdlg_near.suprx`, `libSceNearUtil.suprx`, and `near_profile.suprx` are present. These may be relevant for Vita Near/local-social behavior, but Near is not evidence of arbitrary raw 802.11 control.
- `vitadump/vs0:/sys/external/adhoc_matching.suprx` and `pspnet_adhoc.suprx` are present and map to documented Vita adhoc APIs. These are more promising for app-level Vita-to-Vita discovery than for custom Wi-Fi management advertisements.

## Wiki Dump Pointers

The local HENkaku XML dump contains useful pages for future RE:

- `SceWlanBt`: WLAN/Bluetooth module exports and known config APIs.
- `SceNetPs`: kernel network stack, `netdev_t`, and syscall wrappers.
- `SceSblFwLoader`: includes a code snippet showing `SceWlanBt` loading `os0:kd/wlanbt_robin_img_ax.skprx`.
- `Robin`: identifies the WLAN/Bluetooth hardware and firmware image context.
- `SceSdif`: likely needed when tracing SDIO communication with the Robin module.

## Follow-Up Collection Targets

- Load `SceWlanBt.bootfs.elf` and `SceNetPs.bootfs.elf` into Ghidra. Start from the command/error strings in `SceWlanBt` and the device send/receive strings in `SceNetPs`.
- Consider splitting all 57 embedded `bootimage.skprx.elf` ELFs into named files once a robust module-name extractor is written.
- Load `vitadump/ux0:/FAGDec/kd/wlanbt_robin_img_ax.skprx.elf` into Ghidra; the HENkaku wiki says Robin firmware uses ARMv5TE.
- Keep a copy of the exact firmware version for all modules, because NIDs and internal structures can drift across Vita firmware versions.
