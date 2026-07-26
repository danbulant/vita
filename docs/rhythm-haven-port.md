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
