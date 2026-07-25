# Rhythm Heaven Vita device test

This is the hardware gate for the `YLZE` port. A successful build is not a
successful boot; record the first visible frame, audio state, and the log for
every run.

## Prerequisites

- kubridge 0.3.1 or newer is installed and enabled.
- `libshacccg.suprx` is installed.
- VitaShell FTP is enabled when using the helper script.
- `VITA_IP` is set; `VITA_PORT` defaults to 1337.

The supplied Vita dump already has `ur0:data/libshacccg.suprx` and references
`ur0:tai/kubridge.skprx` from `ur0:tai/config.txt`. Their presence in an old
dump does not prove the live device is identical. The build also stages DSVita's
pinned `v0.3.1_hotfix` kubridge under `dist/rhythm-heaven-vita/prerequisites/`.
If replacement is necessary, upload it to a harmless staging location with:

```sh
scripts/vita-rhythm-heaven.sh prerequisites
```

Then install it manually. The helper deliberately does not overwrite a kernel
plugin or edit taiHEN configuration.

Build the diagnostic package first:

```sh
DSVITA_PROFILE=release-debug scripts/build-rhythm-heaven-vita.sh
```

The diagnostic profile is optimized, but retains debug assertions, overflow
checks, symbols, panic backtraces, and last-interpreted-instruction reporting.
Upload it and the ROM while VitaShell FTP is active:

```sh
scripts/vita-rhythm-heaven.sh deploy
```

Install `ux0:data/DSVita-debug.vpk` in VitaShell. The ROM is uploaded separately
to `ux0:data/dsvita/Rhythm Heaven.nds`; it is never embedded in the VPK.

## Test order

1. Launch DSVita and confirm Rhythm Heaven appears with a red `Recommended`
   button. This validates the little-endian `YLZ` game-code match.
2. Select the recommendation and start with `SoundHle` ARM7 emulation.
3. Record whether the title reaches the Nintendo logo, title screen, and first
   playable rhythm prompt. Note audio presence and obvious cue drift.
4. If it hangs or audio is missing, retry with `AccurateLle` and disable the
   `HLE OS irq handler`.
5. Only after those compatibility modes fail, retry `Hle` for comparison.

Do not evaluate screen rotation, touch ergonomics, or latency until a stable
first playable frame exists.

## Retrieve evidence

Enable VitaShell FTP again, then run:

```sh
scripts/vita-rhythm-heaven.sh logs
```

This writes the device log to ignored `artifacts/rhythm-heaven-vita/log.txt`.
If the Vita reports a coredump, retrieve it with the repository-standard tool:

```sh
cargo vita coredump
```

For each run, preserve these facts alongside the log:

- VPK profile and commit.
- ARM7 mode and HLE OS irq-handler setting.
- Last visible screen or whether the display stayed black.
- Whether music or sound effects played.
- Approximate seconds before hang or crash.
