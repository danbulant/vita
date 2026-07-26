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

### 2026-07-25: Vita3K launch probe

Nixpkgs provides Vita3K build 3821. Its command-line firmware installer
successfully parsed the supplied `PSP2UPDAT.PUP` as firmware 3.65 (build 570279)
and extracted it to an isolated temporary prefix. This establishes that the
Vita dump contains a usable update package; no 3DS/CIA tool is relevant to it.

Vita3K did not provide a game-boot signal on this host. Console mode exits
during initialization without a display, and an Xvfb/OpenGL run segfaults before
guest loading. More importantly, DSVita relies on kubridge fast-memory behavior
and `libshacccg.suprx`, so a Vita3K success would still be weaker than a real
device test. Do not treat this host-emulator failure as a DSVita or Rhythm
Heaven failure.

The packaging script now performs a cheap repeatable VPK smoke check after every
build: `eboot.bin`, `sce_sys/param.sfo`, and the icon must be present, and no DS,
3DS, or CIA game image may be embedded in the VPK.

### 2026-07-25: device diagnostic handoff

No Vita is currently attached over USB. A `VITA_IP` is configured in the shell,
but the common VitaShell FTP ports (21, 1337, and 2121) and SSH port 22 were not
reachable during this session.

`DSVITA_PROFILE=release-debug scripts/build-rhythm-heaven-vita.sh` now produces
and validates `dist/rhythm-heaven-vita/DSVita-debug.vpk`. The build succeeded.
Unlike the stripped release VPK, this optimized diagnostic profile retains the
checks and reporting needed to localize an early ARM/JIT failure.

See `docs/rhythm-heaven-device-test.md` for the mode matrix and
`scripts/vita-rhythm-heaven.sh` for VitaShell FTP deployment/log retrieval.

The Vita dump prerequisite audit found `ur0:data/libshacccg.suprx` and an active
`ur0:tai/kubridge.skprx` entry in `ur0:tai/config.txt`. DSVita's kubridge
submodule is pinned at `a4ef20f` (`v0.3.1_hotfix`), and its freshly built plugin
is now staged outside the VPK for optional manual installation. The helper will
only upload it to `ux0:data/rhythm-heaven-prerequisites`; it never overwrites
the live kernel plugin or edits taiHEN configuration.

Run:

```sh
cargo test -p nds-arm-runtime
cargo run -p nds-arm-runtime --bin nds-inspect -- roms/3588\ -\ Rhythm\ Heaven\ \(US\)\(XenoPhobia\).nds
```

Next translation slices are load/store and block transfer, an address-space
interface for main RAM/TCM/MMIO, condition evaluation, and an ARMv7 emitter with
an instruction-cache flush. ARM7 audio execution and DS 2D graphics command
translation will follow behind those interfaces.

### 2026-07-26: Vita3K desktop bring-up and first guest crash

The earlier Vita3K failure was a host-display problem, not a guest result. The
shell runs inside `screen` and did not inherit the active Hyprland display.
Hyprland was reachable at `/run/user/1000/wayland-1`, but it had no monitor.
Creating a temporary headless output named `VITA3K` made the emulator window
available for remote inspection. Native Wayland connected, but rendered a
black UI with this Vita3K/SDL combination. XWayland (`DISPLAY=:0` and
`SDL_VIDEODRIVER=x11`) rendered correctly.

The working host environment is:

```sh
export DISPLAY=:0
export XDG_SESSION_TYPE=x11
export SDL_VIDEODRIVER=x11
export DRI_PRIME=1
```

`DRI_PRIME=1` is important on this hybrid-GPU host. Native Wayland otherwise
selected llvmpipe after failing to initialize the NVIDIA device. XWayland used
the NVIDIA RTX 5070 directly and reported OpenGL 4.6. Both are host details,
not Vita compatibility requirements.

Vita3K build 3821 required its interactive first-run flow, a user profile,
automatic user login, update checks disabled, and the Vita lock screen dragged
up before its desktop became usable. Firmware 3.65 from the supplied PUP was
already installed in the isolated prefix. The release VPK then installed as
`DSVITA000`, and the staged ROM remained separate at
`ux0:data/dsvita/Rhythm Heaven.nds`.

This produced the first real emulator execution evidence. DSVita loaded its
SELF and firmware modules, initialized vitaGL, reached its own startup logging,
and then failed while allocating the fixed guest-register pages:

```text
[actual_main] Checking for kubridge
Unimplemented _vshKernelSearchModuleByName import called.
Import function for NID 0x2EF7C290 not found
thread 'actual_main' panicked at src/lib.rs:407:138:
called Result::unwrap() on an Err value: Kind(AddrNotAvailable)
```

NID `0x2EF7C290` is DSVita's `kuKernelAllocMemBlock` import. `Mmap::rw` uses
kubridge's kernel allocation options to request exact addresses for emulated
ARM7 and ARM9 register state. Vita3K build 3821 does not provide that module,
so the call fails before the ROM or either DS CPU starts. Missing
`libshacccg.suprx` is also logged, but vitaGL continued past that probe and the
kubridge allocation is the observed fatal gate.

An existing upstream solution is in progress: Vita3K pull request 3958,
`modules: introduce virtual modules (kubridge, fd_fix)`, adds a virtual
kubridge module and a `kuKernelAllocMemBlock` implementation. The PR head
examined here is `d66ef47`; it is not included in packaged build 3821. The next
emulator experiment should build that PR and reuse this installed prefix. If
its allocation semantics are sufficient, DSVita should advance far enough to
exercise the actual ARM translator. If not, compare the requested fixed base
and returned block base before changing DSVita's memory model.

Sources checked:

- [Vita3K repository](https://github.com/Vita3K/Vita3K)
- [Vita3K pull requests](https://github.com/Vita3K/Vita3K/pulls)
- DSVita's checked-in `src/mmap/vita.rs` and generated kubridge stub

The headless output is temporary compositor state and can be removed with
`hyprctl output remove VITA3K` after testing.

### 2026-07-26: kubridge branch and allocator breakthrough

Vita3K PR 3958 (`d66ef47`) was built locally with its virtual `kubridge` and
`fd_fix` modules. The branch uses a newer Qt configuration schema than build
3821, so it needed a fresh config pointed at the existing isolated firmware
and app prefix. On NixOS the locally linked binary also needed its runtime
RPATH patched; XWayland remained the reliable display path.

The virtual module resolved NID `0x2EF7C290`, but its first fixed allocation at
`0xA0000000` still failed. Instrumenting Vita3K's allocator identified the
conflict: DSVita requests a 256 MiB newlib heap, while `alloc_aligned` reserved
almost 512 MiB and retained its unused alignment tail. In addition,
`ksceKernelAllocMemBlock` promoted alignment to `size & -size`, incorrectly
forcing the power-of-two heap to 256 MiB alignment. Together these choices
placed/reserved the heap across DSVita's fixed ARM register and JIT windows.

The experimental fix does two things:

- releases both front and tail padding from aligned allocations and records
  only the requested page count;
- honors the memory-block type/explicit alignment instead of deriving an
  additional alignment from allocation size.

The reusable source patch is `patches/vita3k-dsvita-memory.patch`. It applies
on top of PR 3958. Vita3K's 13 bitmap allocator tests pass with it.

With both fixes, DSVita successfully allocated `0xA0000000`, protected the
32 MiB JIT window at `0x98000000`, reserved its 256 MiB and 176 MiB guest
regions, found and opened `Rhythm Heaven.nds`, initialized configuration and
networking, and reached its first render/shader request. This is the first run
past DSVita's ARM translator/JIT initialization in Vita3K.

Copying the already-supplied dump's `libshacccg.suprx` into the isolated
Vita3K `ur0:data` path allowed shader compiler loading, but executing that LLE
module produced a flood of invalid `ldrex` accesses around `0x84164e6c` and
`0x84248500`, followed by a Vita3K host segfault. The next emulator target is
therefore shader compiler LLE/HLE behavior (or precompiled vitaGL shaders),
not kubridge or DSVita's fixed memory map.

### 2026-07-26: stable frontend and first translated game code

The apparent `libshacccg` failure was a secondary cleanup crash. DSVita started
its RetroAchievements HTTP worker unconditionally; Reqwest/Tokio then panicked
because Vita3K's `sceNetEpollWait` returns `EINVAL`. Unwinding released the
256 MiB newlib heap while shader/compiler threads were still active, producing
the misleading invalid `ldrex` flood. No ready-made replacement translator was
found in Vita3K: its `SceShaccCg` exports are stubs, while the dumped
`libshacccg.suprx` is the available LLE implementation.

DSVita now has an explicit `vita3k` Cargo feature. It suppresses the unsupported
network worker and, when exactly one ROM is installed, launches it directly so
desktop automation does not depend on Vita controller focus. Normal hardware
builds retain networking and the ROM menu. Build this diagnostic package with:

```sh
DSVITA_VITA3K=1 scripts/build-rhythm-heaven-vita.sh
```

With the feature enabled, the supplied shader compiler generated and cached
working vitaGL vertex and fragment programs, the frontend remained stable at
60 FPS, and Rhythm Heaven reached its real boot path. DSVita parsed the ROM,
reported ARM9 entry `0x02000800` and ARM7 entry `0x02380000`, and emitted code
for both processors. This confirms that the current port is executing the
ARM-to-ARM translator, rather than merely displaying a ROM browser.

The first game-code panic was a missing ARM7 MMIO read for DS register
`SOUNDBIAS` at `0x04000504`. The write path and SPU state already existed; the
read table still contained `todo!()`. It now returns the stored 10-bit sound
bias. The next run immediately reached the two sound-capture destination
registers (`0x510` and `0x518`), whose write-side state also already existed;
their read handlers now return that state as well.

Vita3K PR 3958 also logs that kubridge `baseBlock` mirrors are not implemented.
The current run advances through initial JIT generation despite that warning,
so it is not the immediate failure, but correct mirrored mappings may become
necessary as more translated blocks are invalidated or recycled.

### 2026-07-26: shared kubridge mappings and sustained game execution

The next two ARM7 failures were ordinary missing DS hardware handlers rather
than translator instructions. `0x04100010` is the shared game-card data port;
the ARM9 table already called `cartridge_get_rom_data_in`, while ARM7 still had
`todo!()`. ARM7 now uses the same cartridge implementation. The only remaining
`todo!()` in its main I/O table was the `HALTCNT` write at `0x04000301`; the
existing `cpu_set_halt_cnt` implementation is now connected there.

After those fixes the process stopped panicking, but translated execution
walked through low/null addresses. This confirmed that PR 3958's missing
`baseBlock` behavior was no longer optional. DSVita allocates one shared memory
block and repeatedly maps its pages into ARM7/ARM9 read, write, and code-cache
windows. Treating every reserved window as independent memory lets JIT setup
finish but gives the generated code unrelated data.

The Vita3K patch now implements Linux guest aliases using a sparse `memfd`
backing for the 4 GiB Vita address space. `kuKernelMemCommit` with
`KU_KERNEL_MEM_COMMIT_ATTR_HAS_BASE` remaps the requested virtual range with
`MAP_SHARED | MAP_FIXED` at the base block's file offset. Each alias therefore
has a distinct host virtual address but shares physical data, which is also
important for DSVita's address-specific `mprotect`/abort-based JIT invalidation.
Decommit restores the range's original file offset. The page-table renderer has
an equivalent page-table alias path, although that renderer currently crashes
inside the dumped shader compiler and is not used for this test.

With the normal `double-buffer` renderer, the shared mapping implementation
created all requested B/C-region mirrors, detected Nitro SDK 4.2.30001, copied
the cartridge header, translated the ARM9 and ARM7 binaries, initialized the DS
GPU shaders, and then sustained the guest at 58-59 Vita frames per second for
more than 30 seconds without a Rust panic or invalid low-address memory reads.
The DSVita statistics report about 98% of the requested 59/60 DS frames. Both
DS displays are still black, so this is active game execution but not yet a
visually complete boot. The next investigation should distinguish an emulated
CPU/IRQ startup stall from a framebuffer upload or presentation issue.

The updated `patches/vita3k-dsvita-memory.patch` applies cleanly to Vita3K PR
3958 head `d66ef47`. Linux is the validated alias backend; the helper currently
returns unsupported for direct-memory Windows builds rather than pretending
to create independent pages.

### 2026-07-26: black-screen boundary isolated to frozen DS video state

The ROM itself is a known-good baseline. Its SHA-256 is
`6c1889d2f6f5814cb3f5a87b3ed095cf6f3287f011d9cfd60d31ad56b85e038d`.
melonDS 1.1 decrypts its secure area and reaches the animated Rhythm Heaven
title/touch screen in under eight seconds with the software renderer. The
remaining DSVita black screen is therefore not a bad dump or an expected game
delay.

The existing guest-time and final-frame diagnostics depended on environment
variables, which Vita3K does not pass into a Vita application. A `vita3k`-only
diagnostic path now samples frames 60 and 300 automatically. Normal Vita builds
are unchanged. On Vita, final framebuffer sampling uses vitaGL's bound-texture
pointer; `glReadPixels` is not usable through this Vita3K path and caused a
large series of misleading low-address `MemoryRead` errors.

Both samples are identical:

```text
VBLANKHASH#60  main=5480eb98 vram=fc4e4637 palettes=c26ab74e oam=e8e9e4ab regs2d=85f6413c/f4c05819 pow=01
VBLANKHASH#300 main=5480eb98 vram=fc4e4637 palettes=c26ab74e oam=e8e9e4ab regs2d=85f6413c/f4c05819 pow=01
UBODUMP#60/300 disp_cnt[0]=0 disp_cnt[96]=0 bg_cnt[0..4]=0,0,0,0 ofs[0..4]=0,0,0,0
FRAMEDUMP#60/300 hash=5aa01a83 ubo_a=98968312 ubo_b=98968312
```

This rules out the final VitaGL composition as the primary fault. DS vblank
events continue at roughly full speed, but main RAM, VRAM, palettes, OAM, both
2D register files, render-feed snapshots, and the final texture are unchanged
from frame 60 through frame 300. The display control tables remain disabled.
The next target is the emulated ARM9/ARM7 halt and interrupt state during this
interval, especially whether boot code entered HALT waiting for an IRQ that is
pending but not delivered. No large renderer or translator rewrite is
justified before that state is measured.

For unattended desktop runs, the isolated Vita3K configuration must set
`warn-missing-firmware: false`; otherwise its missing font-package modal blocks
the `--installed-path DSVITA000` auto-boot. Vita3K works through Hyprland using
the XCB backend on the temporary `VITA3K` output.

### 2026-07-26: ARM9 IPC stall traced to Vita3K kubridge abort compatibility

CPU and IPC probes narrowed the frozen boot to the standard dual-CPU startup
handshake. ARM7 repeatedly writes output nibble 7 to IPCSYNC. ARM9 remains in
the routine at `0x02034168..0x020341bc`, whose disassembly reads IPCSYNC at
`0x04000180`, echoes the input nibble with `strh`, and waits for it to change.
ARM7 enters the emulator's IPC read/write handlers, but translated ARM9 memory
operations initially did not. At frames 60 and 300 the visible state was still
unchanged, ARM9 IPCSYNC was `0x0007`, and ARM7 IPCSYNC was `0x0700`.

The first JIT correction commits a dynamic branch target and its Thumb state
before an ARM7 scheduler exit. This changed ARM7 CPSR from the inconsistent
Thumb value `0x200000bf` to ARM state `0x2000009f`, but did not by itself pass
the IPC handshake. Release JIT scheduler call sites now also supply their real
guest PC instead of compiling it out with debug logging; this is needed for
correct interrupt/return diagnostics and handling.

The decisive host evidence is Vita3K's missing import:

```text
Import function for NID 0x799F5648 not found
```

NID `0x799F5648` is kubridge 0.3.x `kuKernelRegisterAbortHandler`, which DSVita
uses to turn protected fastmem faults into JIT slow-memory patches. The tested
Vita3K PR implemented `kuKernelRegisterExceptionHandler`, but not this legacy
entry point. Consequently its protected ARM9 access raised a host fault but
never called DSVita's patcher. Adding the exact export and NID changes the run
from a silent black-screen loop to an actual dispatched data abort:

```text
kuKernelRegisterAbortHandler: handler=0x81009E35 old=0x00000000
DABT handler=0x81009E35 FAR=0xB4000208 PC=0x98000074
```

Vita3K's current abort trampoline is also ABI-incomplete. It constructed only
68 bytes, while kubridge's `KuKernelAbortContext` is 344 bytes (16 integer
registers, 32 64-bit VFP registers, and six status/fault words), and it treated
the callback as notification-only instead of applying the handler's edited PC
to retry the patched instruction. Correcting the layout makes DSVita receive a
coherent FAR, exposing the remaining callback-return failure around Vita3K's
halt sentinel. This is now the immediate blocker; it is in Vita3K's kubridge
exception bridge, not the DS IPC implementation or Rhythm Heaven game code.

Architecture references used to verify the handshake and IPCSYNC bit layout:

- [GBATEK IPCSYNC documentation](https://mgba-emu.github.io/gbatek/)
- [NDS boot and dual-CPU overview](https://blog.gistre.epita.fr/posts/augustin.claude-2025-06-16-the_booting_process_of_the_nds_and_its_dual_cpu_architecture/)
- [Nintendo DS boot sequence notes](https://shonumi.github.io/articles/art3.html)

The Vita3K path remains a desktop correctness oracle. Real Vita hardware uses
kubridge's native abort machinery and should keep the fast ARM-to-ARM JIT; do
not replace it with an all-slow-memory renderer or interpreter based on this
host-emulator limitation.

### 2026-07-26: Vita3K reaches the translated IPC loop

The callback-return failure was two separate Dynarmic integration gaps. First,
Vita3K must stop Dynarmic with `HaltReason::MemoryAbort`, not its generic user
halt. Second, the A32 JIT must enable `check_halt_on_memory_access`; that option
emits a checkpoint after each memory operation which records the current guest
instruction PC before returning to Vita3K. Without it, the abort context held
the end-of-block PC (`0x98000074`) and DSVita could not find the metadata for
the faulting translated instruction.

With both changes, the same access produces this verified sequence:

```text
DABT handler=0x81009E35 FAR=0xB4000208 PC=0x98000040
vita3k abort #0 cpu=ARM9 addr=b4000208 pc=98000038 handled=true
IPCSYNCWRITE#0 cpu=ARM7 mask=ffff value=0800
VBLANKHASH#60 ... arm9=pc:02034198 ...
```

This proves that Vita3K now passes a coherent legacy kubridge abort context to
DSVita, DSVita patches the corresponding ARM-to-ARM JIT memory operation, and
both emulated DS CPUs continue into the startup IPC/VBlank loop. The screen is
still unchanged at frame 300. ARM9 reads the remote nibble as 7 but remains in
the polling routine at `0x02034168..0x020341bc`.

The next compatibility gap is protection lifetime. Vita3K's
`handle_access_violation` temporarily unprotects and removes an entire protected
segment after one host fault. Native kubridge can return directly to code that
the abort handler patched, while Vita3K delays the guest callback until after
Dynarmic exits. A simple immediate page re-protect was tested and rejected: it
also trapped Vita3K/DSVita slow-handler accesses and did not advance ARM9. The
next implementation should preserve the original protection metadata and
distinguish translated fastmem accesses from emulator-side accesses, rather
than globally applying `mprotect` again after the callback.

### 2026-07-26: persistent protection prototype and translator alias boundary

DSVita calls `kuKernelFlushCaches` after rewriting a generated ARM instruction.
Vita3K previously treated this as a host CPU cache no-op, but its guest Dynarmic
cache can still contain the old instruction. The compatibility patch now maps
that export to `KernelState::invalidate_jit_cache` for the supplied guest range.
This is required for self-modifying guest JIT code, although it did not pass the
Rhythm Heaven IPC handshake by itself.

A Vita3K-only persistent-protection prototype retained protection metadata,
made the underlying host mappings accessible to emulator code, disabled
Dynarmic fastmem, and checked guest reads/writes in Dynarmic callbacks. It
successfully dispatched and returned from the first ARM9 abort, proving that
the approach can distinguish translated accesses from DSVita's slow handlers.
It is not yet included in the maintained patch: after DSVita installed its full
page set, execution reached a pre-existing translator failure also present in
older Vita3K runs.

Symbolizing guest PC `0x81017e14` against the release ELF identifies
`jump_to_other_guest_pc`. Its incoming in-block byte delta was `0x8000001f` and
using it directly indexes outside the block's `GuestInstOffset` vector before
eventually reading address zero. A narrow attempt to clear bit 31 before the
index calculation was rebuilt and tested. It selected a wrong offset earlier,
corrupted DSVita state before abort registration, then ended in an invalid
exclusive-access loop and Vita3K SIGSEGV. The bit is therefore not a disposable
tag at this boundary; a future fix must recover the canonical target and block
page from metadata before calculating their instruction delta.

Useful reproduction command:

```sh
nix-shell -p binutils --run \
  'addr2line -afiCe target/armv7-sony-vita-newlibeabihf/release/dsvita.elf 0x81017e14'
# Result: dsvita::jit::jit_asm::jump_to_other_guest_pc<ARM7>
```

The software-protection experiment remains outside the checked-in Vita3K patch
until the alias correction is rebuilt and shown to advance the actual title.

### 2026-07-26: ARM32 mid-block dispatch and ITCM mirrors

The failing ARM32 helper assumed that an entry PC mismatch could be converted
directly into a dense `GuestInstOffset` index. The observed delta
`0x8000001f` violated that assumption and let generated code read beyond the
metadata vector. The helper now records each instruction's guest PC and
resolves the requested target explicitly. A missing target produces a bounded
diagnostic instead of corrupting host state.

That diagnostic exposed the concrete alias: ARM9 requested `0x000084d0`, while
the compiled block metadata covered `0x00000004..0x000007cc`. ARM9 ITCM is 32
KiB, so `0x84d0` is the mirror of `0x04d0`. DSVita's AArch64 backend already
folded ITCM entry PCs with `ITCM_SIZE - 1`; the ARM32 resolver now applies the
same CPU-specific rule. It also preserves the existing top-nibble DS memory
mirror collapse and checks the ARM/Thumb tag before entering a cached block.

The Vita release build succeeds with the Vita3K feature. In the persistent
software-protection Vita3K experiment, the corrected build passes the former
`0x84d0` panic, completes the large protection-page installation, and reaches
an ARM7 IPCSYNC read. Roughly seven seconds later it enters a new bad state at
guest PC `0x810062dc`, reads `0x00000e58`, and then branches to PC zero. This is
later than the prior failure and is now the next translator/runtime boundary;
the ITCM fold itself is retained as a verified correction.

### 2026-07-26: split ARM7/ARM9 alternate-entry policy

A release-active JIT-entry guard showed that the later `0x00000e58` read was
not itself a code pointer. ARM7's saved PC had already become `0x0a96472c`;
the JIT map then derived slot `0x00000e58` from that invalid guest PC. Focused
metadata logging traced the corruption back to repeated ARM7 mid-block entry
at `0x02380034`.

Replacing all ARM32 mid-block restores with fresh compilation was safe for
ARM7 but unsuitable for ARM9. ARM9 blocks crossed the 32 KiB ITCM boundary and
continually replaced the shared mirror slots, compiling `0x84d0`, `0x8ca0`,
`0x9470`, and so on. Canonicalizing only the block start still allowed decoded
ranges to overlap and thrash the table.

The validated policy is therefore CPU-specific:

- ARM7 unwinds the containing block and compiles an exact alternate entry,
  avoiding fragile allocator-state restoration.
- ARM9 retains metadata restoration and explicitly folds ITCM mirrors, which
  is required to enter loops inside the shared physical ITCM image.
- JIT table dispatch now rejects invalid low slot and entry pointers before an
  indirect call can amplify corrupted state.

Under Vita3K's persistent software-protection experiment this split build
passes both earlier failures, remains alive, and reaches `VBLANKHASH#60` plus
`FRAMEDUMP#60`. The rendered DS framebuffer is still black and no later frame
milestone was observed during the test window, so game boot is not complete;
the next investigation starts from the now-stable VBlank loop rather than a
host crash.

### 2026-07-26: black-frame ARM9 state trace

The frame-60 register probe confirms that the black output is genuine guest
state rather than a Vita3K presentation failure. Both display-control banks,
interrupt masks, and IPC registers remain zero. ARM7 is executing valid code
near `0x02380028`, but ARM9 is walking zero-filled low memory near
`0x00000fa0`; the loaded ROM header's real ARM9 entry is `0x02000800`.

Release-active route probes ruled out a normal translated immediate branch,
register branch, exception vector, or JIT helper call to low memory. At startup
ARM9's saved PC is correctly `0x02000800`. After 1838 scheduler cycles it is
exactly `0x00000000` before the next top-level `execute()` call, which then
compiles sequential zero blocks (`0x00000000`, `0x000007d0`, `0x00000fa0`,
and onward). This also rules out the earlier theory that only the
`0x02000000` region bits were lost from a valid ITCM target.

Temporary guards around ARM7 execution did not report ARM7 changing the ARM9
PC directly, so they were removed rather than retained as a masking fix. Two
instrumented Vita3K runs also hit a separate nondeterministic host SIGSEGV in
an invalid exclusive-access loop before returning enough inner-JIT state; no
new crash workaround was committed. The next useful trace point is the ARM9
guest-context exit itself: record the last translated block/indirect PC store
that precedes the top-level return, without adding logging on the hot return
path.

### 2026-07-26: Vita3K canonical-memory execution path

The ARM9 zero-PC transition was a `pop {r3,pc}` at `0x02031270`. Its stack
return word was already zero even though LR and the translator's shadow return
stack both held `0x020310cc`. A targeted trace of the matching compiled
`push {r3,lr}` proved that LR was correct in its host register but the write
through DSVita's mirrored fast-memory mapping was not visible through the
canonical DS memory view. This identifies Vita3K kubridge alias coherence, not
ARM decode or ROM state, as the source of the false return.

The Vita3K feature now keeps ordinary blocks in DSVita's existing interpreter
(`INTERP_THRESHOLD = 255`), while real Vita builds retain the ARM-to-ARM JIT.
ARM9 HLE/overlay gates still compile where required. ARM7's OS IRQ handler no
longer gets forced through the compiler: unlike ARM9 it has no HLE replacement,
so the old gate was unnecessary. With those changes Rhythm Heaven passes the
zero-PC loop and performs both ARM7 and ARM9 IPCSYNC initialization writes.

The next abort came from the ARM9 HLE IRQ handler itself. It used raw
`mmu_tcm_addr + guest_addr` pointer dereferences for the IRQ stack and function
table. Those accesses occur outside translated code and therefore cannot use
the JIT abort patcher. They now use `mem_write`/`mem_read`, allowing execution
to advance into game GX FIFO writes. The remaining crash is another Vita3K
kubridge mapping: `FastFixedFifo` uses a double-mapped `VirtualMem` ring, and
its native FIFO address (`0x8044d000` in the observed run) is unmapped when the
first geometry commands arrive. A portable mirrored-buffer fallback is the
next implementation target.

### 2026-07-26: first rendered game boot in Vita3K

`FastFixedFifo` now has a Vita3K-only ordinary allocation backend. It allocates
two FIFO spans and mirrors every insertion into both halves, preserving the
contiguous wraparound window that the geometry command consumer expects without
using kubridge double mappings. Real Vita and non-Vita3K builds keep the
original `Shm`/`VirtualMem` implementation and its SIMD bulk-copy path.

This removes the native GX FIFO abort. The run remains alive through at least
frame 300, with both CPUs halted normally waiting for enabled interrupts,
nonzero display registers (`DISPCNT 0x0021101c`), and changing main-memory,
VRAM, palette, OAM, and renderer UBO hashes. A Hyprland capture shows actual
Rhythm Heaven artwork and text in the DS viewport. Composition is visibly
incorrect (dark tiles, displaced/rotated strips, and a partially assembled
graphic), and the interpreter profile runs around 3-4 FPS, but the title has
advanced from boot firmware/IPC into rendered game code.

The next phase is renderer correctness followed by selective ARM32 JIT
re-enablement. The interpreter should remain the Vita3K reference path until
Vita3K's kubridge aliases become coherent; individual safe blocks or memory
operations can then be promoted back to the ARM-to-ARM translator and compared
against the canonical-memory frame hashes.

### 2026-07-26: Vita3K renderer-backend comparison

The final Vita framebuffer cannot currently be used as a CPU-side diagnostic in
Vita3K. `vglGetTexDataPointer` returns the VitaGL texture allocation, but after
GXM rendering the allocation is not synchronized back from the host GPU. A
Vita3K-only dump of the 960x544 allocation contained only one nonzero byte even
while the host window visibly showed game artwork. This also explains the
stable `5aa01a83` frame hash: it describes stale CPU-visible storage, not the
presented image. The probe was removed after confirming this limitation.

The same build was tested with both Vita3K rendering backends under Hyprland:

- Vulkan reaches recognizable Rhythm Heaven artwork, but with displaced strips,
  missing/dark tiles, and incorrect composition. Vita3K logs `Mask not
  implemented in the vulkan renderer!` while retrieving the VitaGL shaders.
- OpenGL is substantially worse: after game startup it shows a purple rectangle
  and horizontal scanlines instead of recognizable artwork. Its translated
  shaders compile, with many warnings about parameters and temporary registers
  potentially being used before initialization.

Vulkan therefore remains the reference Vita3K backend. Web and upstream-history
searches did not find a ready-made ARM7/ARM9-to-ARMv7 translator or a specific
fix for this VitaGL rendering failure. Vita3K itself still describes the project
as experimental, and its current pull-request list includes ongoing VitaGL
compatibility work (base-vertex and ETC1 support), so backend gaps remain an
active upstream area:

- <https://github.com/Vita3K/Vita3K>
- <https://github.com/Vita3K/Vita3K/pulls>

The next useful discriminator is a renderer-stage capture before VitaGL/GXM, or
a reduced 2D test that checks texture upload, integer texture sampling, uniform
layout, and draw geometry independently. Host screenshots are authoritative for
Vita3K until explicit GXM surface readback is implemented.

The existing `Gpu2DSoftRenderer` was also tested as a possible shader-path
bypass. Enabling it makes DSVita compile its `blend_new` Vita shader pair.
Vita3K segfaults in its shader handling immediately after writing the newly
compiled fragment shader to cache (`Unhandled access to 0x0`), before game
execution begins. The experiment was reverted. The software renderer is not a
drop-in Vita3K workaround without first reducing or correcting that shader.

### 2026-07-26: bounded Vita3K texture backing

The apparent title-screen corruption was traced to VitaGL texture remapping and
Vita3K's 1,024-entry texture cache. DSVita calls `vglRemapTexPtr` for roughly a
dozen VRAM, palette, OAM, and blend textures per frame. Each remap gives the GXM
texture a fresh data address, so Vita3K creates hundreds of cache identities in
seconds. Increasing the disposable Vita3K build's cache to 4,096 entries delayed
the block corruption until the larger cache began recycling. Disabling the
texture cache produced an entirely black viewport.

Two additional experiments established the update semantics:

- Keeping a single backing address rendered the initial contents but did not
  expose later CPU writes to Vita3K.
- Reusing two addresses avoided unbounded cache growth and rendered a clean,
  rotated Nintendo logo. Re-uploading cached textures on every bind in the
  disposable Vita3K build did not make the later game image appear.

DSVita's Vita3K feature now uses `vglCycleTexPtr`, a bounded VitaGL helper that
cycles each logical texture through five persistent mapped allocations. Five is
one more than VitaGL's four-frame deferred-free window, so queued GPU work is not
overwritten while the number of Vita3K cache identities remains bounded. Real
Vita builds continue to use `vglRemapTexPtr`. This removes the stale/displaced
strips and prevents cache churn from manufacturing a misleading old title
image. The authoritative current output is a clean Nintendo logo followed by a
black viewport through frame 300 and beyond.

The black viewport is interactive rather than a CPU stall. An XWayland mouse
press reached DSVita through Vita3K's front-touch emulation as raw Vita
coordinates `(1442,560)`, mapped by DSVita to `(721,282)` in its 960x544 layout.
At frame 300, the untouched run had `main=d113cf71`, `oam=63e81d4a`; the touched
run had `main=bcc02c82`, `oam=8759ddfd`, while VRAM, palettes, display registers,
CPU PCs, and IRQ state matched. At frame 600 main RAM continued changing while
the touched OAM state remained stable. The prompt consumed input and both CPUs
continued their normal IRQ wait loop; Vita3K is failing to present later direct
texture updates, rather than the game failing to boot past the logo.
