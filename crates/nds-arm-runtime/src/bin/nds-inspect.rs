use nds_arm_runtime::{arm, nds::NdsHeader};
use std::{env, fs, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("nds-inspect: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("usage: nds-inspect GAME.nds")?;
    let rom = fs::read(path)?;
    let header = NdsHeader::parse(&rom)?;
    eprintln!("title: {}", header.title);
    eprintln!("game code: {}", header.game_code);
    eprintln!(
        "ARM9: ROM {:#010x}, load {:#010x}, entry {:#010x}, {} bytes",
        header.arm9_rom_offset, header.arm9_load, header.arm9_entry, header.arm9_size
    );
    eprintln!(
        "ARM7: ROM {:#010x}, load {:#010x}, entry {:#010x}, {} bytes",
        header.arm7_rom_offset, header.arm7_load, header.arm7_entry, header.arm7_size
    );
    let entry_offset = header
        .arm9_entry
        .checked_sub(header.arm9_load)
        .ok_or("ARM9 entry precedes load address")? as usize;
    let arm9 = header.arm9(&rom);
    if entry_offset >= arm9.len() {
        return Err("ARM9 entry lies outside image".into());
    }
    for instruction in arm::decode_block(&arm9[entry_offset..], header.arm9_entry, 16) {
        eprintln!("{instruction:?}");
    }
    Ok(())
}
