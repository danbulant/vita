//! ARMv5TE A32 decoder and translation IR.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Condition {
    Eq,
    Ne,
    Cs,
    Cc,
    Mi,
    Pl,
    Vs,
    Vc,
    Hi,
    Ls,
    Ge,
    Lt,
    Gt,
    Le,
    Always,
    Never,
}

impl Condition {
    fn decode(value: u32) -> Self {
        use Condition::*;
        [
            Eq, Ne, Cs, Cc, Mi, Pl, Vs, Vc, Hi, Ls, Ge, Lt, Gt, Le, Always, Never,
        ][value as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AluOp {
    And,
    Eor,
    Sub,
    Rsb,
    Add,
    Adc,
    Sbc,
    Rsc,
    Tst,
    Teq,
    Cmp,
    Cmn,
    Orr,
    Mov,
    Bic,
    Mvn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operand2 {
    Immediate { value: u32, carry: Option<bool> },
    RegisterShiftImmediate { rm: u8, shift_type: u8, amount: u8 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferSize {
    Byte,
    Half,
    Word,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Addressing {
    pub rn: u8,
    pub offset: u32,
    pub add: bool,
    pub pre_index: bool,
    pub write_back: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Instruction {
    DataProcessing {
        condition: Condition,
        op: AluOp,
        set_flags: bool,
        rn: u8,
        rd: u8,
        operand2: Operand2,
    },
    Transfer {
        condition: Condition,
        load: bool,
        size: TransferSize,
        signed: bool,
        rd: u8,
        address: Addressing,
    },
    Branch {
        condition: Condition,
        link: bool,
        target: u32,
    },
    SoftwareInterrupt {
        condition: Condition,
        comment: u32,
    },
    Unsupported {
        address: u32,
        word: u32,
    },
}

pub fn decode_a32(address: u32, word: u32) -> Instruction {
    let condition = Condition::decode(word >> 28);
    if word & 0x0e00_0000 == 0x0a00_0000 {
        let displacement = (((word & 0x00ff_ffff) << 2) as i32) << 6 >> 6;
        return Instruction::Branch {
            condition,
            link: word & (1 << 24) != 0,
            target: address.wrapping_add(8).wrapping_add(displacement as u32),
        };
    }
    if word & 0x0f00_0000 == 0x0f00_0000 {
        return Instruction::SoftwareInterrupt {
            condition,
            comment: word & 0x00ff_ffff,
        };
    }
    // ARMv4+ halfword and signed data transfer, immediate offset form.
    if word & 0x0e00_0090 == 0x0000_0090 && word & (1 << 22) != 0 {
        let kind = (word >> 5) & 3;
        let load = word & (1 << 20) != 0;
        if kind != 0 && (load || kind == 1) {
            return Instruction::Transfer {
                condition,
                load,
                size: if kind == 2 {
                    TransferSize::Byte
                } else {
                    TransferSize::Half
                },
                signed: kind >= 2,
                rd: ((word >> 12) & 15) as u8,
                address: Addressing {
                    rn: ((word >> 16) & 15) as u8,
                    offset: ((word >> 4) & 0xf0) | (word & 0x0f),
                    add: word & (1 << 23) != 0,
                    pre_index: word & (1 << 24) != 0,
                    write_back: word & (1 << 21) != 0,
                },
            };
        }
    }
    // ARM single data transfer with an immediate offset. Register offsets are
    // left explicit until the shifter is shared with data-processing decode.
    if word & 0x0c00_0000 == 0x0400_0000 && word & (1 << 25) == 0 {
        return Instruction::Transfer {
            condition,
            load: word & (1 << 20) != 0,
            size: if word & (1 << 22) != 0 {
                TransferSize::Byte
            } else {
                TransferSize::Word
            },
            signed: false,
            rd: ((word >> 12) & 15) as u8,
            address: Addressing {
                rn: ((word >> 16) & 15) as u8,
                offset: word & 0xfff,
                add: word & (1 << 23) != 0,
                pre_index: word & (1 << 24) != 0,
                write_back: word & (1 << 21) != 0,
            },
        };
    }
    if word & 0x0c00_0000 == 0 {
        let op = [
            AluOp::And,
            AluOp::Eor,
            AluOp::Sub,
            AluOp::Rsb,
            AluOp::Add,
            AluOp::Adc,
            AluOp::Sbc,
            AluOp::Rsc,
            AluOp::Tst,
            AluOp::Teq,
            AluOp::Cmp,
            AluOp::Cmn,
            AluOp::Orr,
            AluOp::Mov,
            AluOp::Bic,
            AluOp::Mvn,
        ][((word >> 21) & 15) as usize];
        let operand2 = if word & (1 << 25) != 0 {
            let rotate = ((word >> 8) & 15) * 2;
            let value = (word & 255).rotate_right(rotate);
            Operand2::Immediate {
                value,
                carry: (rotate != 0).then_some(value >> 31 != 0),
            }
        } else if word & (1 << 4) == 0 {
            Operand2::RegisterShiftImmediate {
                rm: (word & 15) as u8,
                shift_type: ((word >> 5) & 3) as u8,
                amount: ((word >> 7) & 31) as u8,
            }
        } else {
            return Instruction::Unsupported { address, word };
        };
        return Instruction::DataProcessing {
            condition,
            op,
            set_flags: word & (1 << 20) != 0,
            rn: ((word >> 16) & 15) as u8,
            rd: ((word >> 12) & 15) as u8,
            operand2,
        };
    }
    Instruction::Unsupported { address, word }
}

pub fn decode_block(bytes: &[u8], address: u32, max_instructions: usize) -> Vec<Instruction> {
    let mut result = Vec::new();
    for (index, chunk) in bytes.chunks_exact(4).take(max_instructions).enumerate() {
        let pc = address.wrapping_add((index * 4) as u32);
        let instruction = decode_a32(pc, u32::from_le_bytes(chunk.try_into().unwrap()));
        let terminal = matches!(
            instruction,
            Instruction::Branch { .. } | Instruction::SoftwareInterrupt { .. }
        );
        result.push(instruction);
        if terminal {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_arm9_startup_instruction_classes() {
        assert_eq!(
            decode_a32(0x0200_0000, 0xe3a0_0001),
            Instruction::DataProcessing {
                condition: Condition::Always,
                op: AluOp::Mov,
                set_flags: false,
                rn: 0,
                rd: 0,
                operand2: Operand2::Immediate {
                    value: 1,
                    carry: None
                },
            }
        );
        assert_eq!(
            decode_a32(0x0200_0804, 0xe58c_c208),
            Instruction::Transfer {
                condition: Condition::Always,
                load: false,
                size: TransferSize::Word,
                signed: false,
                rd: 12,
                address: Addressing {
                    rn: 12,
                    offset: 0x208,
                    add: true,
                    pre_index: true,
                    write_back: false,
                },
            }
        );
        assert_eq!(
            decode_a32(0x0200_0808, 0xe1dc_00b6),
            Instruction::Transfer {
                condition: Condition::Always,
                load: true,
                size: TransferSize::Half,
                signed: false,
                rd: 0,
                address: Addressing {
                    rn: 12,
                    offset: 6,
                    add: true,
                    pre_index: true,
                    write_back: false,
                },
            }
        );
        assert_eq!(
            decode_a32(0x0200_0004, 0xeaff_fffd),
            Instruction::Branch {
                condition: Condition::Always,
                link: false,
                target: 0x0200_0000,
            }
        );
        assert_eq!(
            decode_a32(0, 0xef00_0005),
            Instruction::SoftwareInterrupt {
                condition: Condition::Always,
                comment: 5,
            }
        );
    }

    #[test]
    fn block_stops_at_control_flow() {
        let words = [0xe3a0_0001u32, 0xeaff_fffd, 0xe3a0_0002];
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        assert_eq!(decode_block(&bytes, 0x0200_0000, 32).len(), 2);
    }
}
