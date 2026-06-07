A repo for vita experiments, mainly a GUI audio player in `crates/mpvrs`.

To check if a rust project builds for vita, use `cargo vita build vpk --release`.
`cargo vita coredump` can also retrieve coredumps, if mentioned from logs - you will be provided with logs if relevant.

Use stderr for logs.

The vita has 4 ARM cores with ~300MB usable RAM.
