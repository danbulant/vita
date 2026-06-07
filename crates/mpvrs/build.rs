use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../third_party/vita_sqlite_vfs/vita_sqlite.c");
    println!("cargo:rerun-if-changed=../../third_party/vita_sqlite_vfs/sqlite3.h");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("sony-vita") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let object = out_dir.join("vita_sqlite.o");
    let compiler = cc::Build::new().get_compiler();
    let status = Command::new(compiler.path())
        .args(compiler.args())
        .arg("-std=c99")
        .arg("-I../../third_party/vita_sqlite_vfs")
        .arg("-c")
        .arg("../../third_party/vita_sqlite_vfs/vita_sqlite.c")
        .arg("-o")
        .arg(&object)
        .status()
        .expect("failed to run C compiler for Vita SQLite VFS");

    if !status.success() {
        panic!("failed to compile Vita SQLite VFS");
    }

    println!("cargo:rustc-link-arg={}", object.display());
}
