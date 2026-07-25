# Rhythm Heaven Vita port

This file is the persistent engineering log. Update it whenever ROM evidence,
research, architecture, or a test changes the plan.

## 2026-07-25 - ROM identification and translator bootstrap

### Workspace evidence

- `file` identifies the supplied image as a decrypted Nintendo DS ROM titled
  `RHYTHMHEAVEN`, product code `YLZE01`, revision 0.
- The parsed header says ARM9 is loaded at `0x02000000`, enters at
  `0x02000800`, and is 342936 bytes. ARM7 loads and enters at `0x02380000`
  and is 159528 bytes.
- The first ARM9 instructions are `mov r12, #0x04000000`, a word store to
  `[r12, #0x208]`, a halfword load from `[r12, #6]`, a compare, and a
  conditional loop. This is direct DS I/O initialization, not portable game
  logic. The values were obtained from the ROM by `nds-inspect`; they are not
  assumptions.
- The repository already has a working Rust/Vita SDK environment through
  `nix develop` and `cargo-vita`. Existing uncommitted `mpvrs` font work is
  unrelated and must remain untouched.

### Research notes and sources

- The DS ARM9 address space maps main RAM at `0x02000000`, I/O at
  `0x04000000`, palettes at `0x05000000`, several VRAM windows beginning at
  `0x06000000`, OAM at `0x07000000`, and BIOS at `0xffff0000`. The register
  at entry-point offset `0x208` is therefore in the DS I/O region. See the
  [GBATEK DS memory and I/O maps](https://mgba-emu.github.io/gbatek/).
- A second independent early hardware reference also places base I/O at
  `0x04000000`; it is useful for cross-checking uncertain registers:
  [Stephen Stair's DS Hardware Reference](https://www.akkit.org/info/dsref.htm).
- BlocksDS documents that an NDS image packages separate ARM9 and ARM7
  programs, supporting the decision to model two CPU images rather than a
  single executable: [BlocksDS build process](https://blocksds.skylyrac.net/docs/internal/build_process/).

These are reverse-engineered community references, not official Nintendo
documentation. Register behavior remains provisional until confirmed against
another emulator implementation or an execution trace.

### Architecture decision

Use a staged dynamic binary translator:

1. Parse the NDS container with strict bounds checks.
2. Decode ARMv5TE A32/Thumb instructions into a console-neutral IR.
3. Run the IR through a portable interpreter first, with an explicit DS bus.
4. Replace hot basic blocks with ARMv7 A32 emitted into Vita executable memory.
5. Lower DS BIOS, CP15, and MMIO operations to compatibility calls; never emit
   them as native Vita memory or coprocessor operations.

The shared A32 heritage helps the final emitter, but does not eliminate binary
translation. DS virtual addresses and hardware effects must be preserved, and
the ARM7 program must eventually run or be replaced by equivalent audio and
service behavior.

### Implemented and verified

- `crates/nds-arm-runtime`: bounds-checked header/ARM image parsing.
- A32 data-processing immediate/register-immediate-shift, branch, SWI, word and
  byte immediate transfer, and immediate halfword/signed transfer decoding.
- Basic blocks terminate at branches and SWIs.
- Initial host test suite and `nds-inspect` utility.

### Open questions

- Whether Vita user applications can obtain executable writable pages directly
  or should use vitaShaRK for JIT memory and instruction-cache maintenance.
- Which ARM7 services Rhythm Heaven uses before reaching its title screen.
- Whether the game mostly uses the DS 2D engines (likely, but not yet measured)
  and which graphics register subset is needed for first visible output.
- Where overlays begin and which game state first leaves SDK startup code.

### Next evidence gates

- Decode the full reachable startup path and report instruction coverage.
- Add the DS bus with main RAM, I/O traps, and deterministic unmapped-access
  errors; interpret through the first I/O polling loop.
- Parse FAT/FNT and overlay tables so code discovery includes loaded overlays.
- Only then scaffold the Vita app around the runtime and produce a VPK.

## 2026-07-25 - Existing translator survey and DSVita pivot

The earlier "write a translator" plan was superseded after searching current
projects. Keep `nds-arm-runtime` as a small independent ROM/decode test tool,
but do not grow it into another emulator unless DSVita proves unusable.

### Survey result

- [DSVita](https://github.com/Grarak/DSVita) is an active Rust/C++ DS emulator
  explicitly optimized for ARM32 and Vita. Release 0.9.4 was published on
  2026-07-21. Its repository documents an ARM/Thumb JIT, cold-block
  interpreter, ARM7 HLE/SoundHLE/LLE choices, fastmem through kubridge, mostly
  complete 2D rendering, and vitaGL-assisted graphics. This is the selected
  base and is pinned at commit `ae36f93a231751c4b0ef2f6672d50e93a3a91dca`
  in `third_party/DSVita`.
- [NooDS](https://github.com/Hydr8gon/NooDS) has an older Vita port and is
  DSVita's stated semantic reference. It remains useful for differential
  execution, but DSVita has the more relevant current ARM32 JIT and Vita fast
  memory work.
- [DeSmuME-Vita](https://github.com/masterfeizz/DeSmuME-Vita) already proved a
  Vita DS JIT possible, but it is much older and historical reports describe
  low performance. It is not the primary base.
- No title-specific Rhythm Heaven report was found in DSVita's indexed issues
  or compatibility pages. Compatibility must be tested on hardware rather than
  inferred from the general "most games run" statement.

DSVita is GPL-3.0. Any distributed modified build must continue to satisfy that
license. The ROM remains user-provided and is excluded from git.

### Reproducible build work

The repository Nix shell now provides:

- LLVM 18 libclang plus its resource headers for bindgen.
- Unwrapped clang/clang++ 21 and llvm-ar compatibility names expected by
  DSVita, plus lld 21.
- A combined read-only Vita SDK tree containing VitaSDK, OpenSSL, ImGui,
  vitaGL, vitaShaRK, math-neon, SceShaccCgExt, and taiHEN artifacts.
- CMake for building DSVita's kubridge and streaming stubs.

`cargo vita build vpk --release` completes and produced
`third_party/DSVita/target/armv7-sony-vita-newlibeabihf/release/dsvita.vpk`.
The VPK is a generated artifact and must not be committed.

Run `scripts/build-rhythm-heaven-vita.sh` to verify the `YLZE` ROM header,
build the VPK, and stage this deployable layout under ignored `dist/`:

```text
rhythm-heaven-vita/
|-- DSVita.vpk
`-- ux0/data/dsvita/Rhythm Heaven.nds
```

The Vita requires `libshacccg.suprx` and kubridge 0.3.1 or newer. Install the
VPK, copy the staged `ux0` tree, then launch the ROM from DSVita. Hardware logs
are written under `ux0:data/dsvita/log/`.

### Immediate test concern

Rhythm Heaven is timing-sensitive and expects the DS held sideways with heavy
touch/flick input. "Boots" should first mean reaching stable rendered game
code with audio; input orientation and latency are follow-up quality gates.
Test ARM7 modes in this order if boot stalls: HLE, SoundHLE, AccurateLLE.

The supplied `YLZE` image is an unencrypted Nintendo DS ROM, not a 3DS CIA.
It does not need the 3DS NAND dump or CIA decryption tools.

The DS ARM946E-S and Vita Cortex-A9 both execute A32 instructions, but the ROM
cannot be called as Vita code. It expects the DS memory map, BIOS, interrupts,
coprocessor state, graphics engines, sound hardware, and a second ARM7 CPU.
`nds-arm-runtime` therefore decodes ARMv5TE into an intermediate form. The same
front end can feed a portable interpreter during bring-up and a Vita ARMv7 code
emitter later. BIOS calls and memory-mapped IO remain explicit compatibility
hooks instead of becoming unsafe native accesses.

Current milestone:

1. Bounds-checked NDS header and ARM image loader.
2. A32 data-processing, branch, and software-interrupt decoding.
3. Basic-block formation with host tests.
4. `nds-inspect` verifies a ROM and decodes its ARM9 entry block.

### 2026-07-25: first title compatibility hook

DSVita already contains the production translator needed for this port: its
ARM/Thumb decoder feeds an ARM32 JIT, while cold or unsupported blocks can run
through an interpreter and later hand back to compiled code. This is a more
mature version of the same interpreter-plus-emitter structure prototyped in
`nds-arm-runtime`, so title work should extend DSVita rather than grow a second
complete emulator.

Added a `YLZ` compatibility entry for Rhythm Heaven. DSVita stores the four-byte
game code as a little-endian `u32` and masks off the region byte, so `YLZE` maps
to key `0x5A4C59` and the entry also covers Japanese and European revisions. It
recommends SoundHLE as the initial compromise: the game is audio/timing-heavy,
while full ARM7 LLE costs more CPU. AccurateLLE remains the first fallback when
hardware testing shows drift, missing cues, or a boot hang.

Verification after this change:

- `cargo test -p nds-arm-runtime`: 3 passed.
- `cargo vita build vpk --release` in `third_party/DSVita`: succeeded.
- The rebuilt VPK contains no ROM and remains an ignored generated artifact.

This still does not prove that the title reaches its first rendered frame. That
requires a Vita run (or an ARM Linux execution harness) and the resulting log.
The next invasive changes must be based on that evidence, not speculative JIT
rewrites.

Run:

```sh
cargo test -p nds-arm-runtime
cargo run -p nds-arm-runtime --bin nds-inspect -- roms/3588\ -\ Rhythm\ Heaven\ \(US\)\(XenoPhobia\).nds
```

Next translation slices are load/store and block transfer, an address-space
interface for main RAM/TCM/MMIO, condition evaluation, and an ARMv7 emitter with
an instruction-cache flush. ARM7 audio execution and DS 2D graphics command
translation will follow behind those interfaces.
