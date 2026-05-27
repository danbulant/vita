# PS Vita Raw 802.11 Feasibility

## Short Answer

There is no currently documented userland VitaSDK or HENkaku-wiki API that can send arbitrary raw 802.11 management frames from the PS Vita. The public network API can create raw IP sockets, but that is layer 3 (`AF_INET`) and does not expose Wi-Fi management frames such as beacons, probe requests, probe responses, action frames, or vendor-specific information elements.

Custom StreetPass-like advertisements would therefore require new kernel/firmware reverse engineering rather than normal homebrew networking code.

## Evidence From This Dump

The local dump contains Wi-Fi-relevant components. After running FAGDec, several of them now have decrypted ELF output under `vitadump/ux0:/FAGDec`:

- `vitadump/ur0:/tai/boot_config.txt` has load entries for `os0:kd/net_ps.skprx` and `os0:kd/wlanbt.skprx`, but these files are not present as normal files in the local `os0:/kd` dump. The HENkaku wiki says decrypted `os0:kd/bootimage.skprx` contains a list of ELFs whose library paths are reported as `bootfs:`. Its module list identifies `SceNetPs` as `bootfs:net_ps.skprx` and `SceWlanBt` as `bootfs:wlanbt.skprx` on firmware 3.60, which explains why VitaShell/FAGDec did not see them as ordinary `os0:/kd` files.
- `vitadump/os0:/kd/wlanbt_robin_img_ax.skprx` starts with the Vita `SCE\0` SELF header. FAGDec produced `vitadump/ux0:/FAGDec/kd/wlanbt_robin_img_ax.skprx.elf`, which `file` identifies as `ELF 32-bit LSB ARM, EABI5`. `readelf` reports no sections, so this looks like a loadable firmware image rather than a normal symbolized module.
- Strings from the decrypted Robin firmware include `SceWlanBtRobinImageAx`, `wlan`, `sdio`, `amp_sdio`, `bt_sdio`, `MAC Tx`, `MAC Tx Notify`, `TxMgmt80211MsgQ`, `MacMgmt80211MsgQ`, `MacMgmtSMEMsgQ`, `MAC Mgmt`, `Marvell Micro AP`, and `$Id: w8787-Ax, RF878X, FP65, 14.65.9.p223, BT_SDIO $`. This is strong evidence that the blob contains Marvell WLAN/BT firmware code and 802.11 management queues.
- `vitadump/vs0:/sys/external/libnet.suprx`, `libnetctl.suprx`, `pspnet_adhoc.suprx`, and `adhoc_matching.suprx` also appear as `SCE\0` SELF containers with no useful wireless strings from a plain `strings` pass.
- FAGDec produced decrypted ELFs for the userland net libraries at `vitadump/ux0:/FAGDec/sys/external/`. Strings from those include `wlan0`, `wlan1`, `wlan2`, `Type:Wlan`, `SceNetCtlPeriodicalScan`, adhoc auth/matching names, and `(raw,%2d)`, but still no public 802.11 management-frame injection API.
- FAGDec also produced `vitadump/ux0:/FAGDec/kd/bootimage.skprx.elf`. This decrypted bootimage contains 57 embedded ELF headers. The target bootfs modules were extracted locally to `vitadump/ux0:/FAGDec/kd/bootimage_modules/SceNetPs.bootfs.elf` and `vitadump/ux0:/FAGDec/kd/bootimage_modules/SceWlanBt.bootfs.elf`.
- `SceNetPs` was embedded at bootimage file offset `0xd01ec`, size `0x3a530`. `SceWlanBt` was embedded at file offset `0x165b14`, size `0xe548`.
- `SceWlanBt.bootfs.elf` strings include `SceWlanBtRobinWlanCommand`, `SceWlanBtRobinWlan`, `SceWlanBtRobinWlanSdioAggrRx`, `SceNetDrvWlan`, `SceWlanBtRobinWlanWakeConfig`, `SceWlanBtRobinWlanContext`, and error logs for command/response IDs. This is the main host-driver target for finding the Robin command path.
- `vitadump/ur0:/tai/config.txt` shows installed network-adjacent plugins: `kubridge.skprx`, `NoPowerLimits.skprx`, and `net_logging_mgr.skprx`. None of the available dumped plugin strings exposed raw 802.11 terminology.

## Public API Surface

VitaSDK headers show normal IP networking only:

- `psp2/net/net.h` exposes `sceNetSocket`, `sceNetSendto`, `sceNetRecvfrom`, etc.
- `psp2common/net.h` defines `SCE_NET_SOCK_RAW = 3`, but only defines `SCE_NET_AF_INET = 2`. This supports raw IP protocols such as ICMP/IP-header work, not Ethernet or 802.11 frame injection.
- `psp2/net/netctl.h` exposes connection state and information such as SSID, BSSID, RSSI, channel, and current MAC address via `sceNetCtlInetGetInfo`; it does not expose scan-frame construction or transmission.
- `psp2/net/adhoc_matching.h` exposes game adhoc matching with hello options and peer selection. This is useful for Vita-to-Vita application-level discovery, but it is not a raw 802.11 management-frame interface.

The HENkaku wiki dump agrees with that split:

- `SceWlanBt` exports `SceWlanBtForDriver` and `SceWlan`. Known APIs include `sceWlanSetConfiguration`, `sceWlanGetConfiguration`, callback registration, and guessed monitor attach/detach functions. The documented config values are WLAN on/off, flight mode on/off, and possibly power saving.
- `SceNetPs` has an internal `netdev_t` with driver callbacks such as `fnc_tx_pkt`, `fnc_ioctl`, `fnc_pkt_rx`, and `sceNetRegisterDeviceForDriver`. This is below userland sockets and is the likely integration point between the OS IP stack and a device driver, but the documented structure does not prove access to raw 802.11 management frames.

## Hardware/Firmware Context

The HENkaku wiki `Robin` page identifies the Vita WLAN/Bluetooth module as Marvell 88W878S-BKB2 on PCH-1XXX, likely SD8787-derived, connected over SDIO. It points to the Linux `mwifiex` driver as related prior art. The wiki also states:

- Robin firmware uses ARMv5TE.
- The firmware image is stored in `wlanbt_robin_img_ax.skprx` starting at offset 305 on firmware 3.60.

This matters because many Marvell fullmac-style chips perform 802.11 management in firmware. If the Vita stack follows that model, the host driver may submit high-level commands rather than arbitrary raw 802.11 bytes. StreetPass-like probe requests or beacons would then need either an undocumented host command, a firmware patch, or replacement firmware behavior.

## StreetPass Comparison

Detailed 3DS StreetPass notes now live in `docs/3ds-streetpass.md`. For Vita purposes, the key comparison is that classic StreetPass requires control over probe requests, probe responses, Nintendo vendor action frames, and CCMP-encrypted 802.11 data frames. The documented Vita APIs do not provide that control.

## Feasibility Assessment

Confidence: medium-high that normal homebrew cannot do raw 802.11 injection with existing public APIs.

Confidence: medium that a kernel plugin alone may still not be sufficient unless an internal WLAN driver command path can be found. Ghidra analysis confirms `SceWlanBt` has a host-to-Robin command/response path, but the visible commands analyzed so far look like scan/join/control commands rather than arbitrary management-frame transmit primitives.

Likely routes, from easiest to hardest:

1. Use another Wi-Fi device for raw frame work and let the Vita communicate over IP/Bluetooth/USB. This is practical now.
2. Use Vita adhoc APIs for Vita-to-Vita application discovery, accepting that frames are Sony/Vita-defined and not arbitrary 802.11 advertisements.
3. Reverse `SceWlanBt` and the Robin firmware command protocol to find whether a host command can send custom probe/action frames or vendor IEs.
4. Patch Robin firmware or replace parts of the WLAN driver path to add a custom management-frame transmit primitive.

## Ghidra Findings

Analysis was performed on the extracted bootfs modules and Robin firmware in the `VitaWifiGhidra` project.

Renamed/commented `SceWlanBt.bootfs.elf` functions:

- `InitializeWlanDriverContext` at `0x8100940c`: creates `SceWlanBtRobinWlanContext`, initializes Robin WLAN state, registers `SceNetDrvWlan`, and starts the Robin worker thread.
- `InitializeRobinWlanQueues` at `0x8100271c`: creates event flag/mutex objects named `SceWlanBtRobinWlanCommand`, allocates a 0x1000-byte `SceWlanBtRobinWlan` buffer, and a 0x4000-byte `SceWlanBtRobinWlanSdioAggrRx` buffer.
- `HandleRobinWlanRxMessage` at `0x81001cd4`: central RX/message handler. Message type `1` is a command response. It expects `response_cmd == queued_cmd | 0x8000` and a matching sequence byte; nonzero result sets error `0x80418004`. Message type `0` carries RX packet data; subtype `0x02` is direct packet delivery and subtype `0xE6` parses aggregated descriptors.
- `QueueRobinCommandAsync` at `0x81002db4`: queues a command buffer and signals `SceWlanBtRobinWlanCommand`.
- `SendRobinCommandSync` at `0x81002f44`: queues a command and waits up to 10 seconds for the response event bit.
- `BuildAndSendRobinScanCommand` at `0x81006a6c`: builds command id `0x06`. It constructs TLV-like scan parameters for BSSID/SSID/channel list and supports optional extra data up to `0x400` bytes.
- `BuildAndSendRobinJoinCommand` at `0x81004b10`: builds command id `0x12`. It constructs join/association-style fields including SSID, channel, supported rates, and several IE-like elements such as tags `0x01`, `0x03`, `0x1f`, `0x2d`, `0x30`, and vendor tag `0xdd`.
- `SendRobinSimpleCommand28` at `0x81003514`: sends command id `0x28` with a small 2-byte payload.
- `TransmitQueuedRobinCommand` at `0x810030c8`: transmits the current queued command through the lower send primitive when state permits.
- `HandleWlanRxInterrupt` at `0x81003144`: receives status/interrupt data, updates flags, and calls `PollRobinWlanRxPackets` when RX is pending.

`SceNetPs.bootfs.elf` observations:

- `SceNetPs` initializes kernel packet/thread memory pools named `SceNetKernelDevSend`, `SceNetKernelDevRecv`, `SceNetKernelPkt`, etc.
- Interface lookup/config paths reference `wlan0`, but the lower-level WLAN command path lives in `SceWlanBt` via `SceNetDrvWlan` registration.
- No obvious `SceNetPs` raw 802.11 injection path was found in this first pass; it looks like IP-stack/netdev plumbing.

Robin firmware observations:

- Imported `wlanbt_robin_img_ax.skprx.elf` has no functions auto-created by Ghidra yet, but strings include `MacMgmtSMEMsgQ`, `Marvell Micro AP`, and the firmware ID string `$Id: w8787-Ax, RF878X, FP65, 14.65.9.p223, BT_SDIO $`.

Implication: there is a real command interface to the Marvell/Robin firmware. The best short-term path is to map command ids around `0x06` scan and `0x12` join and determine whether any command accepts arbitrary/custom IEs or management frames. If only scan/join IE construction is possible, it may allow limited custom IE behavior in normal firmware-controlled frames, but not StreetPass-style arbitrary probe/action frame injection.

## RE Plan For A Native Implementation

1. Load the extracted bootfs modules into Ghidra: `vitadump/ux0:/FAGDec/kd/bootimage_modules/SceWlanBt.bootfs.elf` and `SceNetPs.bootfs.elf`. Both are valid ARM ELFs but stripped and have no section table.
2. Load `vitadump/ux0:/FAGDec/kd/wlanbt_robin_img_ax.skprx.elf` into Ghidra as ARM little-endian firmware. It is also a valid ARM ELF but has no section table, so manual load analysis may be needed.
3. Label known imports/exports from the HENkaku wiki: `SceWlanBtForDriver`, `SceNetPsForDriver`, `SceSdif`, `SceSblFwLoader`, and `SceWlan`.
4. Trace `SceWlanBt` initialization from firmware load through SDIO setup. The wiki snippet shows `sceSblFwLoaderLockForDriver("os0:kd/wlanbt_robin_img_ax.skprx")` followed by `sceSblFwLoaderLoadForDriver(1, 0, 0x80000, &g_fwLoadedSize)`.
5. In `SceWlanBt`, continue from `BuildAndSendRobinScanCommand` and `BuildAndSendRobinJoinCommand`; document all command ids and payload layouts passed to `SendRobinCommandSync`.
6. Compare command IDs and buffer layouts against Linux `mwifiex` for Marvell SD8787/88W8787.
7. Search for commands related to scan, remain-on-channel, host MLME, management frame TX, custom IE, beacon IE, probe request IE, and action frame TX. The decrypted Robin strings make `TxMgmt80211MsgQ`, `MacMgmt80211MsgQ`, and `MacMgmtSMEMsgQ` good firmware-side starting anchors.
8. If a suitable command exists, prototype a kernel plugin that calls the internal command path and sends a controlled vendor action/probe frame on a fixed channel.
9. If only high-level scan/join commands exist, assess firmware patching. Focus on the Robin ARMv5TE firmware image and the SDIO command mailbox.
10. Validate with an external monitor-mode Wi-Fi adapter and Wireshark. Start with harmless local lab frames: vendor action frames or probe requests with a test OUI/IE, not spoofed third-party networks.

## Open Questions

- Does Vita `SceWlanBt` expose an internal, non-exported management-frame transmit helper?
- Does Robin firmware accept Marvell-style custom IE or hostcmd management frame transmit commands?
- Are `sceWlanBtAttachMonitorForDriver` and `sceWlanBtDetachMonitorForDriver` related to event monitoring/callbacks only, or do they expose RX of lower-level WLAN events?
- Can `SceNetPs` `netdev_t.fnc_ioctl` reach WLAN-specific ioctls useful for channel, scan, or custom IE control?
- What exact `SceWlanBt` command IDs map to scan/join/power/firmware operations, and is any command capable of host-provided management frame TX or custom IE configuration?

## References

- Local dump: `vitadump/ur0:/tai/boot_config.txt`
- Local dump: `vitadump/os0:/kd/wlanbt_robin_img_ax.skprx`
- Local FAGDec output: `vitadump/ux0:/FAGDec/kd/wlanbt_robin_img_ax.skprx.elf`
- Local extracted bootfs modules: `vitadump/ux0:/FAGDec/kd/bootimage_modules/*.bootfs.elf`
- Local FAGDec output: `vitadump/ux0:/FAGDec/sys/external/*.suprx.elf`
- Local wiki dump: `doc-dump/henkaku-wiki.xml`, pages `SceWlanBt`, `SceNetPs`, `SceSblFwLoader`, and `Robin`
- VitaSDK headers: `include/psp2/net/net.h`, `include/psp2common/net.h`, `include/psp2/net/netctl.h`, `include/psp2/net/adhoc_matching.h`
- HENkaku wiki: https://wiki.henkaku.xyz/vita/Main_Page
- VitaSDK: https://vitasdk.org/
