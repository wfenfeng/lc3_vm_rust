use std::{env, io, process};
use std::fs::File;
use std::io::{Read, stdout, Write};
use std::ptr::read;
use crate::Op::TRAP;
use crate::RegisterType::{COND, PC, R0, R7};
use termios::*;

const MEMORY_MAX: usize = 1 << 16;
const REG_COUNT: usize = 10;
const PC_START: u16 = 0x3000;
const MR_KBSR: u16 = 0xFE00;
const MR_KBDR: u16 = 0xFE02;  /* keyboard data */

enum RegisterType {
    R0,
    R1,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
    PC,
    COND,
}

#[derive(Debug)]
enum Op {
    BR,
    ADD,
    LD,
    ST,
    JSR,
    AND,
    LDR,
    STR,
    RTI,
    NOT,
    LDI,
    STI,
    JMP,
    RES,
    LEA,
    TRAP,
    Unknown,
}

// get op enum by op code
fn get_op(op_code: u16) -> Op {
    match op_code {
        0 => Op::BR,
        1 => Op::ADD,
        2 => Op::LD,
        3 => Op::ST,
        4 => Op::JSR,
        5 => Op::AND,
        6 => Op::LDR,
        7 => Op::STR,
        8 => Op::RTI,
        9 => Op::NOT,
        10 => Op::LDI,
        11 => Op::STI,
        12 => Op::JMP,
        13 => Op::RES,
        14 => Op::LEA,
        15 => Op::TRAP,
        _ => Op::Unknown,
    }
}

struct VM {
    regs: Vec<u16>,
    memory: Vec<u16>,
}

impl VM {
    fn new() -> VM {
        let mut vm = VM { regs: vec![0u16; REG_COUNT], memory: vec![0u16; MEMORY_MAX] };
        vm.write_reg(PC, PC_START);
        vm
    }

    // get the register index by register type enum
    fn get_index(t: RegisterType) -> usize {
        match t {
            RegisterType::R0 => 0,
            RegisterType::R1 => 1,
            RegisterType::R2 => 2,
            RegisterType::R3 => 3,
            RegisterType::R4 => 4,
            RegisterType::R5 => 5,
            RegisterType::R6 => 6,
            RegisterType::R7 => 7,
            RegisterType::PC => 8,
            RegisterType::COND => 9,
        }
    }

    // read reg by register type enum
    fn read_reg(&self, t: RegisterType) -> u16 {
        self.regs[Self::get_index(t)]
    }

    // write reg by register type enum
    fn write_reg(&mut self, t: RegisterType, v: u16) {
        self.regs[Self::get_index(t)] = v;
    }

    // read reg by register index
    fn read_reg_by_index(&self, i: u16) -> u16 {
        self.regs[i as usize]
    }

    // write reg by register index
    fn write_reg_by_index(&mut self, i: u16, v: u16) {
        self.regs[i as usize] = v;
    }

    fn read_pc(&mut self) -> u16 {
        self.regs[Self::get_index(PC)]
    }

    // program counter increment 1
    fn add_pc(&mut self) {
        self.regs[Self::get_index(PC)] += 1;
    }

    fn read_memory(&mut self, address: u16) -> u16 {
        if (address == MR_KBSR) {
            if (check_key()) {
                self.write_memory(MR_KBSR, 1 << 15);
                self.write_memory(MR_KBDR, read_char() as u16);
            } else {
                self.write_memory(MR_KBSR, 0);
            }
        }
        self.memory[address as usize]
    }

    // write val to memory
    fn write_memory(&mut self, address: u16, val: u16) {
        self.memory[address as usize] = val;
    }

    // update cond register by given val
    fn update_flags_by_val(&mut self, val: u16) {
        if val == 0 {
            self.write_reg(COND, 1 << 1); // zero
        } else if ((val >> 15) & 0x1) == 1 {
            self.write_reg(COND, 1 << 2); // positive
        } else {
            self.write_reg(COND, 1 << 0); // negative
        }
    }

    // update cond register by Register Index
    fn update_flags_by_index(&mut self, i: u16) {
        let val = self.read_reg_by_index(i);
        self.update_flags_by_val(val);
    }

    // update cond register by RegisterType
    fn update_flags(&mut self, t: RegisterType) {
        let val = self.read_reg(t);
        self.update_flags_by_val(val);
    }
}

// read a char from stdin
fn read_char() -> u8 {
    let mut buf = [0u8];
    io::stdin().read_exact(&mut buf).unwrap();
    buf[0]
}

fn check_key() -> bool {
    true
}

// read the image instruction to the memory
fn read_image(image_path: &str, vm: &mut VM) -> io::Result<()> {
    let mut file = File::open(image_path).expect(&format!("Open image file {} failed!", image_path));
    let mut buf = [0u8; 2];
    // read the first 2 byte, it means that the program start position in memory
    file.read_exact(&mut buf)?;
    let mut origin = (buf[0] as u16) << 8 | buf[1] as u16;
    // println!("start address: {}", origin);

    loop {
        // read 2 byte
        match file.read_exact(&mut buf) {
            Ok(_) => {
                vm.write_memory(origin, (buf[0] as u16) << 8 | buf[1] as u16);
                origin += 1;
            }
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                // finish read file
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }
    }
    // println!("end address: {}", origin);
    Ok(())
}

// if imm is positive or zero, return the imm
// otherwise Pad with 1 from high to low
fn sign_extend(imm: u16, len: i32) -> u16 {
    if ((imm >> len - 1) & 0x1) == 0 {
        imm
    } else {
        imm | (0xffff << len)
    }
}

fn br(vm: &mut VM, instr: u16) {
    let flags = (instr) >> 9 & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    if (flags & vm.read_reg(COND)) != 0 {
        let pc_val = vm.read_pc() as u32;
        vm.write_reg(PC, (pc_val + pc_offset) as u16);
    }
}

fn add(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let r1 = (instr >> 6) & 0x7;
    let imm5_flag = (instr >> 5) & 0x1;
    if imm5_flag == 0 {
        let r2 = instr & 0x7;
        // prevent overflow
        let val1 = vm.read_reg_by_index(r1) as u32;
        let val2 = vm.read_reg_by_index(r2) as u32;
        vm.write_reg_by_index(r0, (val1 + val2) as u16);
    } else {
        let imm5 = sign_extend(instr & 0x1f, 5) as u32;
        vm.write_reg_by_index(r0, (vm.read_reg_by_index(r1) as u32 + imm5) as u16);
    }
    vm.update_flags_by_index(r0);
}

fn ld(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    let mem_add = (vm.read_pc() as u32 + pc_offset) as u16;
    let mem_val = vm.read_memory(mem_add);
    vm.write_reg_by_index(r0, mem_val);
    vm.update_flags_by_index(r0);
}

fn st(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    let mem_add = (vm.read_pc() as u32 + pc_offset) as u16;
    vm.write_memory(mem_add, vm.read_reg_by_index(r0));
}

fn jsr(vm: &mut VM, instr: u16) {
    vm.write_reg(R7, vm.read_reg(PC));
    let flag = (instr >> 11) & 0x1;
    if (flag == 0) {
        let base_r = (instr >> 6) & 0x7;
        vm.write_reg(PC, vm.read_reg_by_index(base_r));
    } else {
        let pc_offset = sign_extend(instr & 0x7ff, 11) as u32;
        vm.write_reg(PC, (vm.read_reg(PC) as u32 + pc_offset) as u16);
    }
}

fn and(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let r1 = (instr >> 6) & 0x7;
    let imm5_flag = (instr >> 5) & 0x1;
    if imm5_flag == 0 {
        let r2 = instr & 0x7;
        vm.write_reg_by_index(r0, vm.read_reg_by_index(r1) & vm.read_reg_by_index(r2));
    } else {
        let imm5 = sign_extend(instr & 0x1f, 5);
        vm.write_reg_by_index(r0, vm.read_reg_by_index(r1) & imm5);
    }
    vm.update_flags_by_index(r0);
}

fn ldr(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let r1 = (instr >> 6) & 0x7;
    let offset = sign_extend(instr & 0x3f, 6) as u32;
    let mem_add = (vm.read_reg_by_index(r1) as u32 + offset) as u16;
    let mem_val = vm.read_memory(mem_add);
    vm.write_reg_by_index(r0, mem_val);
    vm.update_flags_by_index(r0);
}

fn str(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let r1 = (instr >> 6) & 0x7;
    let offset = sign_extend(instr & 0x3f, 6) as u32;
    vm.write_memory((vm.read_reg_by_index(r1) as u32 + offset) as u16, vm.read_reg_by_index(r0));
}

fn not(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let r1 = (instr >> 6) & 0x7;
    vm.write_reg_by_index(r0, !vm.read_reg_by_index(r1));
    vm.update_flags_by_index(r0);
}

fn ldi(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    let first_mem_add = (vm.read_pc() as u32 + pc_offset) as u16;
    let second_mem_add = vm.read_memory(first_mem_add);
    let res = vm.read_memory(second_mem_add);
    vm.write_reg_by_index(r0, res);
    vm.update_flags_by_index(r0);
}

fn sti(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    let read_mem_add = (vm.read_pc() as u32 + pc_offset) as u16;
    let write_mem_add = vm.read_memory(read_mem_add);
    let val = vm.read_reg_by_index(r0);
    vm.write_memory(write_mem_add, val);
}

fn jmp(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 6) & 0x7;
    vm.write_reg(PC, vm.read_reg_by_index(r0));
}

fn lea(vm: &mut VM, instr: u16) {
    let r0 = (instr >> 9) & 0x7;
    let pc_offset = sign_extend(instr & 0x1ff, 9) as u32;
    let val = (vm.read_pc() as u32 + pc_offset) as u16;
    vm.write_reg_by_index(r0, val);
    vm.update_flags_by_index(r0);
}

fn trap(vm: &mut VM, instr: u16) {
    let pc_val = vm.read_pc();
    vm.write_reg(R7, pc_val);
    match instr & 0xff {
        // TRAP_GETC
        0x20 => {
            vm.write_reg(R0, read_char() as u16);
            vm.update_flags(R0);
        }
        // TRAP_OUT
        0x21 => {
            let c = vm.read_reg(R0) as u8;
            print!("{}", c as char);
            stdout().flush().expect("failed to flush");
        }
        // TRAP_PUTS
        0x22 => {
            let mut start = vm.read_reg(R0);
            loop {
                let c = vm.read_memory(start);
                if c == 0 {
                    break;
                }
                print!("{}", (c as u8) as char);
                start += 1;
            }
            stdout().flush().expect("failed to flush");
        }
        // TRAP_IN
        0x23 => {
            print!("Enter a character: ");
            stdout().flush().expect("failed to flush");
            let c = read_char();
            print!("{}", c as char);
            stdout().flush().expect("failed to flush");
            vm.write_reg(R0, c as u16);
            vm.update_flags(R0);
        }
        // TRAP_PUTSP
        0x24 => {
            // Putsp
            let mut start_address = vm.read_reg(R0);
            loop {
                let c = vm.read_memory(start_address);
                if c == 0 {
                    break;
                }
                // low 8 bits to char
                let c1 = ((c & 0xFF) as u8) as char;
                print!("{}", c1);
                // high 8 bits to char
                let c2 = ((c >> 8) as u8) as char;
                if c2 != '\0' {
                    print!("{}", c2);
                }
                start_address += 1;
            }
            stdout().flush().expect("failed to flush");
        }
        // TRAP_HALT
        0x25 => {
            println!("HALT!");
            stdout().flush().expect("failed to flush");
            process::exit(1);
        }
        _ => {}
    }
}


fn main() {
    // terminal setup reference https://github.com/digorithm/LC-3-Rust
    // Some tricks to make the VM's terminal be interactive
    let stdin = 0;
    let termios = Termios::from_fd(stdin).unwrap();

    // make a mutable copy of termios
    // that we will modify
    let mut new_termios = termios.clone();
    new_termios.c_iflag &= IGNBRK | BRKINT | PARMRK | ISTRIP | INLCR | IGNCR | ICRNL | IXON;
    new_termios.c_lflag &= !(ICANON | ECHO); // no echo and canonical mode

    tcsetattr(stdin, TCSANOW, &mut new_termios).unwrap();

    let args: Vec<String> = env::args().collect();
    let mut vm = VM::new();

    if args.len() != 2 {
        panic!("lc3 [image-file]")
    }

    // load instructions to memory from the give image file
    read_image(&args[1], &mut vm).expect("Read image file failed");

    // begin fetch instruction and execute instruction
    loop {
        let pc_val = vm.read_pc();
        vm.add_pc();
        let instr = vm.read_memory(pc_val);
        let op = get_op(instr >> 12);
        match op {
            Op::BR => br(&mut vm, instr),
            Op::ADD => add(&mut vm, instr),
            Op::LD => ld(&mut vm, instr),
            Op::ST => st(&mut vm, instr),
            Op::JSR => jsr(&mut vm, instr),
            Op::AND => and(&mut vm, instr),
            Op::LDR => ldr(&mut vm, instr),
            Op::STR => str(&mut vm, instr),
            Op::NOT => not(&mut vm, instr),
            Op::LDI => ldi(&mut vm, instr),
            Op::STI => sti(&mut vm, instr),
            Op::JMP => jmp(&mut vm, instr),
            Op::LEA => lea(&mut vm, instr),
            Op::TRAP => trap(&mut vm, instr),
            _ => {}
        }
    }
}

