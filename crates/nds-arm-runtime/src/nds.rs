use std::fmt;

const HEADER_LEN: usize = 0x160;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NdsHeader {
    pub title: String,
    pub game_code: String,
    pub arm9_rom_offset: u32,
    pub arm9_entry: u32,
    pub arm9_load: u32,
    pub arm9_size: u32,
    pub arm7_rom_offset: u32,
    pub arm7_entry: u32,
    pub arm7_load: u32,
    pub arm7_size: u32,
    pub fnt_offset: u32,
    pub fnt_size: u32,
    pub fat_offset: u32,
    pub fat_size: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    TruncatedHeader,
    InvalidAscii(&'static str),
    RegionOutsideRom(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedHeader => f.write_str("NDS header is truncated"),
            Self::InvalidAscii(name) => write!(f, "{name} is not ASCII"),
            Self::RegionOutsideRom(name) => write!(f, "{name} lies outside the ROM"),
        }
    }
}

impl std::error::Error for Error {}

fn le32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn ascii_field(data: &[u8], name: &'static str) -> Result<String, Error> {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    if !data[..end].is_ascii() {
        return Err(Error::InvalidAscii(name));
    }
    Ok(String::from_utf8(data[..end].to_vec()).unwrap())
}

fn validate_region(rom: &[u8], offset: u32, size: u32, name: &'static str) -> Result<(), Error> {
    let end = u64::from(offset) + u64::from(size);
    if end > rom.len() as u64 {
        Err(Error::RegionOutsideRom(name))
    } else {
        Ok(())
    }
}

impl NdsHeader {
    pub fn parse(rom: &[u8]) -> Result<Self, Error> {
        if rom.len() < HEADER_LEN {
            return Err(Error::TruncatedHeader);
        }
        let header = Self {
            title: ascii_field(&rom[0x00..0x0c], "title")?,
            game_code: ascii_field(&rom[0x0c..0x10], "game code")?,
            arm9_rom_offset: le32(rom, 0x20),
            arm9_entry: le32(rom, 0x24),
            arm9_load: le32(rom, 0x28),
            arm9_size: le32(rom, 0x2c),
            arm7_rom_offset: le32(rom, 0x30),
            arm7_entry: le32(rom, 0x34),
            arm7_load: le32(rom, 0x38),
            arm7_size: le32(rom, 0x3c),
            fnt_offset: le32(rom, 0x40),
            fnt_size: le32(rom, 0x44),
            fat_offset: le32(rom, 0x48),
            fat_size: le32(rom, 0x4c),
        };
        validate_region(rom, header.arm9_rom_offset, header.arm9_size, "ARM9 image")?;
        validate_region(rom, header.arm7_rom_offset, header.arm7_size, "ARM7 image")?;
        validate_region(rom, header.fnt_offset, header.fnt_size, "file name table")?;
        validate_region(
            rom,
            header.fat_offset,
            header.fat_size,
            "file allocation table",
        )?;
        Ok(header)
    }

    pub fn arm9<'a>(&self, rom: &'a [u8]) -> &'a [u8] {
        &rom[self.arm9_rom_offset as usize..(self.arm9_rom_offset + self.arm9_size) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_bounds_checks_header() {
        let mut rom = vec![0; 0x300];
        rom[..8].copy_from_slice(b"TESTGAME");
        rom[0x0c..0x10].copy_from_slice(b"ABCD");
        rom[0x20..0x24].copy_from_slice(&0x160u32.to_le_bytes());
        rom[0x24..0x28].copy_from_slice(&0x0200_0000u32.to_le_bytes());
        rom[0x28..0x2c].copy_from_slice(&0x0200_0000u32.to_le_bytes());
        rom[0x2c..0x30].copy_from_slice(&4u32.to_le_bytes());
        rom[0x30..0x34].copy_from_slice(&0x170u32.to_le_bytes());
        rom[0x3c..0x40].copy_from_slice(&4u32.to_le_bytes());
        let header = NdsHeader::parse(&rom).unwrap();
        assert_eq!(header.title, "TESTGAME");
        assert_eq!(header.arm9(rom.as_slice()).len(), 4);
    }
}
