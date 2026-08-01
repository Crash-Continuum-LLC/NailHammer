//! Bytecode on the wire.
//!
//! # Why this exists
//!
//! VM-DESIGN.md §8.1: **the plugin boundary is bytes, not Rust types.** A front
//! end turns source into bytecode; a host runs bytecode. Nothing else crosses.
//! Once that is the contract, Rust's lack of a stable ABI stops mattering, two
//! vendored copies of this crate are indistinguishable because neither sees the
//! other's types, and a plugin does not even need the execution engine.
//!
//! # Why it is hand-rolled
//!
//! `nh-vm` depends on nothing, because a language built on it inherits every
//! dependency it has (§8.4). That rules out serde, bincode and postcard — not
//! because they are bad, but because "vendor this and you need no registry" is
//! a property worth more than the few hundred lines below.
//!
//! # The format
//!
//! ```text
//! magic    "NHVM"          4 bytes
//! version  u16             the format, not the VM
//! frame    u32
//! globals  u32
//! fns      u32 count, then (name, addr, arity, frame) each
//! code     u32 count, then one tagged instruction each
//! ```
//!
//! Little-endian and fixed width throughout. Opcode tags are **assigned
//! explicitly and never reused**: adding an instruction takes the next number,
//! so a stream written by an older front end still decodes.
//!
//! The version is checked on load and a mismatch names both sides. That is the
//! whole of the compatibility story — §8.3 argues at length against the config
//! hashes and version ranges an earlier draft proposed, on the grounds that
//! they are enforcement designed for an adversary who does not exist.

use std::collections::HashMap;

use crate::op::{Cmp, Op, Reg};
use crate::program::{FnDef, Program};
use crate::value::Value;

const MAGIC: &[u8; 4] = b"NHVM";

/// The wire format's own version, independent of the VM's.
///
/// Bumped when the encoding changes shape — not when an instruction is added,
/// since a new tag is backward compatible by construction.
pub const FORMAT_VERSION: u16 = 1;

#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    /// Not bytecode at all.
    NotBytecode,
    /// Bytecode from a different format version.
    Version { found: u16, expected: u16 },
    /// Ran out of input. Always an error, never a panic: a truncated stream is
    /// something a host will meet, and it must not take the host down.
    Truncated,
    /// A tag this build has no instruction for.
    UnknownOpcode(u8),
    UnknownTag { what: &'static str, tag: u8 },
    /// Text that was not UTF-8.
    BadString,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::NotBytecode => write!(f, "not NailHammer bytecode"),
            WireError::Version { found, expected } => write!(
                f,
                "bytecode format {found}, but this VM reads {expected}"
            ),
            WireError::Truncated => write!(f, "bytecode ended early"),
            WireError::UnknownOpcode(t) => write!(f, "unknown opcode {t}"),
            WireError::UnknownTag { what, tag } => write!(f, "unknown {what} tag {tag}"),
            WireError::BadString => write!(f, "a string was not valid UTF-8"),
        }
    }
}

impl std::error::Error for WireError {}

/// How a language's own instructions cross the wire.
///
/// A language with extensions has to say how they encode, because nothing else
/// can know. [`crate::NoExt`] implements it unreachably, so a language without
/// extensions writes nothing.
pub trait Wire: Sized {
    fn encode(&self, out: &mut Vec<u8>);
    fn decode(r: &mut Reader<'_>) -> Result<Self, WireError>;
}

impl Wire for crate::op::NoExt {
    fn encode(&self, _out: &mut Vec<u8>) {
        match *self {}
    }
    fn decode(_r: &mut Reader<'_>) -> Result<Self, WireError> {
        // Unreachable in practice: an encoder that has no value of this type
        // never wrote an `Ext` tag for a decoder to find.
        Err(WireError::UnknownOpcode(TAG_EXT))
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.pos.checked_add(n).ok_or(WireError::Truncated)?;
        let s = self.bytes.get(self.pos..end).ok_or(WireError::Truncated)?;
        self.pos = end;
        Ok(s)
    }

    pub fn u8(&mut self) -> Result<u8, WireError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, WireError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32, WireError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub fn usize(&mut self) -> Result<usize, WireError> {
        Ok(self.u32()? as usize)
    }

    pub fn f64(&mut self) -> Result<f64, WireError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub fn reg(&mut self) -> Result<Reg, WireError> {
        self.u16()
    }

    pub fn str(&mut self) -> Result<String, WireError> {
        let n = self.usize()?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| WireError::BadString)
    }
}

pub fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn put_usize(out: &mut Vec<u8>, v: usize) {
    put_u32(out, v as u32);
}
pub fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}
pub fn put_str(out: &mut Vec<u8>, s: &str) {
    put_usize(out, s.len());
    out.extend_from_slice(s.as_bytes());
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

const V_NIL: u8 = 0;
const V_BOOL: u8 = 1;
const V_NUM: u8 = 2;
const V_STR: u8 = 3;

fn put_value(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Nil => out.push(V_NIL),
        Value::Bool(b) => {
            out.push(V_BOOL);
            out.push(*b as u8);
        }
        Value::Num(n) => {
            out.push(V_NUM);
            put_f64(out, *n);
        }
        Value::Str(s) => {
            out.push(V_STR);
            put_str(out, s);
        }
    }
}

fn get_value(r: &mut Reader<'_>) -> Result<Value, WireError> {
    Ok(match r.u8()? {
        V_NIL => Value::Nil,
        V_BOOL => Value::Bool(r.u8()? != 0),
        V_NUM => Value::Num(r.f64()?),
        V_STR => Value::str(&r.str()?),
        tag => return Err(WireError::UnknownTag { what: "value", tag }),
    })
}

// ---------------------------------------------------------------------------
// Instructions
//
// Tags are explicit and permanent. A new instruction takes the next free
// number; none is ever reused, so a stream written before it existed still
// decodes.
// ---------------------------------------------------------------------------

const TAG_LOADK: u8 = 0;
const TAG_MOVE: u8 = 1;
const TAG_ADD: u8 = 2;
const TAG_SUB: u8 = 3;
const TAG_MUL: u8 = 4;
const TAG_DIV: u8 = 5;
const TAG_REM: u8 = 6;
const TAG_POW: u8 = 7;
const TAG_NEG: u8 = 8;
const TAG_NOT: u8 = 9;
const TAG_BITNOT: u8 = 10;
const TAG_AND: u8 = 11;
const TAG_OR: u8 = 12;
const TAG_XOR: u8 = 13;
const TAG_SHL: u8 = 14;
const TAG_SHR: u8 = 15;
const TAG_COMPARE: u8 = 16;
const TAG_LOADGLOBAL: u8 = 17;
const TAG_STOREGLOBAL: u8 = 18;
const TAG_JUMP: u8 = 19;
const TAG_JUMPIFFALSE: u8 = 20;
const TAG_JUMPIFTRUE: u8 = 21;
const TAG_PRINT: u8 = 22;
const TAG_HALT: u8 = 23;
const TAG_AWAIT: u8 = 24;
const TAG_CALL: u8 = 25;
const TAG_RETURN: u8 = 26;
const TAG_RETURNUNIT: u8 = 27;
const TAG_EXT: u8 = 255;

fn cmp_tag(c: Cmp) -> u8 {
    match c {
        Cmp::Eq => 0,
        Cmp::Ne => 1,
        Cmp::Lt => 2,
        Cmp::Le => 3,
        Cmp::Gt => 4,
        Cmp::Ge => 5,
    }
}

fn cmp_of(t: u8) -> Result<Cmp, WireError> {
    Ok(match t {
        0 => Cmp::Eq,
        1 => Cmp::Ne,
        2 => Cmp::Lt,
        3 => Cmp::Le,
        4 => Cmp::Gt,
        5 => Cmp::Ge,
        tag => return Err(WireError::UnknownTag { what: "comparison", tag }),
    })
}

fn put_op<X: Wire>(out: &mut Vec<u8>, op: &Op<X>) {
    macro_rules! bin {
        ($tag:expr, $dst:expr, $a:expr, $b:expr) => {{
            out.push($tag);
            put_u16(out, *$dst);
            put_u16(out, *$a);
            put_u16(out, *$b);
        }};
    }
    macro_rules! un {
        ($tag:expr, $dst:expr, $a:expr) => {{
            out.push($tag);
            put_u16(out, *$dst);
            put_u16(out, *$a);
        }};
    }

    match op {
        Op::LoadK { dst, value } => {
            out.push(TAG_LOADK);
            put_u16(out, *dst);
            put_value(out, value);
        }
        Op::Move { dst, src } => un!(TAG_MOVE, dst, src),
        Op::Add { dst, a, b } => bin!(TAG_ADD, dst, a, b),
        Op::Sub { dst, a, b } => bin!(TAG_SUB, dst, a, b),
        Op::Mul { dst, a, b } => bin!(TAG_MUL, dst, a, b),
        Op::Div { dst, a, b } => bin!(TAG_DIV, dst, a, b),
        Op::Rem { dst, a, b } => bin!(TAG_REM, dst, a, b),
        Op::Pow { dst, a, b } => bin!(TAG_POW, dst, a, b),
        Op::And { dst, a, b } => bin!(TAG_AND, dst, a, b),
        Op::Or { dst, a, b } => bin!(TAG_OR, dst, a, b),
        Op::Xor { dst, a, b } => bin!(TAG_XOR, dst, a, b),
        Op::Shl { dst, a, b } => bin!(TAG_SHL, dst, a, b),
        Op::Shr { dst, a, b } => bin!(TAG_SHR, dst, a, b),
        Op::Neg { dst, a } => un!(TAG_NEG, dst, a),
        Op::Not { dst, a } => un!(TAG_NOT, dst, a),
        Op::BitNot { dst, a } => un!(TAG_BITNOT, dst, a),
        Op::Compare { dst, cmp, a, b } => {
            out.push(TAG_COMPARE);
            out.push(cmp_tag(*cmp));
            put_u16(out, *dst);
            put_u16(out, *a);
            put_u16(out, *b);
        }
        Op::LoadGlobal { dst, slot } => {
            out.push(TAG_LOADGLOBAL);
            put_u16(out, *dst);
            put_u32(out, *slot);
        }
        Op::StoreGlobal { slot, src } => {
            out.push(TAG_STOREGLOBAL);
            put_u32(out, *slot);
            put_u16(out, *src);
        }
        Op::Jump(t) => {
            out.push(TAG_JUMP);
            put_usize(out, *t);
        }
        Op::JumpIfFalse { src, target } => {
            out.push(TAG_JUMPIFFALSE);
            put_u16(out, *src);
            put_usize(out, *target);
        }
        Op::JumpIfTrue { src, target } => {
            out.push(TAG_JUMPIFTRUE);
            put_u16(out, *src);
            put_usize(out, *target);
        }
        Op::Print { src } => {
            out.push(TAG_PRINT);
            put_u16(out, *src);
        }
        Op::Halt => out.push(TAG_HALT),
        Op::Await { dst, src } => un!(TAG_AWAIT, dst, src),
        Op::Call { dst, base, argc, key, shown } => {
            out.push(TAG_CALL);
            put_u16(out, *dst);
            put_u16(out, *base);
            put_usize(out, *argc);
            put_str(out, key);
            put_str(out, shown);
        }
        Op::Return { src } => {
            out.push(TAG_RETURN);
            put_u16(out, *src);
        }
        Op::ReturnUnit => out.push(TAG_RETURNUNIT),
        Op::Ext(x) => {
            out.push(TAG_EXT);
            x.encode(out);
        }
    }
}

fn get_op<X: Wire>(r: &mut Reader<'_>) -> Result<Op<X>, WireError> {
    macro_rules! bin {
        ($ctor:ident) => {{
            let dst = r.reg()?;
            let a = r.reg()?;
            let b = r.reg()?;
            Op::$ctor { dst, a, b }
        }};
    }

    Ok(match r.u8()? {
        TAG_LOADK => Op::LoadK { dst: r.reg()?, value: get_value(r)? },
        TAG_MOVE => Op::Move { dst: r.reg()?, src: r.reg()? },
        TAG_ADD => bin!(Add),
        TAG_SUB => bin!(Sub),
        TAG_MUL => bin!(Mul),
        TAG_DIV => bin!(Div),
        TAG_REM => bin!(Rem),
        TAG_POW => bin!(Pow),
        TAG_AND => bin!(And),
        TAG_OR => bin!(Or),
        TAG_XOR => bin!(Xor),
        TAG_SHL => bin!(Shl),
        TAG_SHR => bin!(Shr),
        TAG_NEG => Op::Neg { dst: r.reg()?, a: r.reg()? },
        TAG_NOT => Op::Not { dst: r.reg()?, a: r.reg()? },
        TAG_BITNOT => Op::BitNot { dst: r.reg()?, a: r.reg()? },
        TAG_COMPARE => {
            let cmp = cmp_of(r.u8()?)?;
            Op::Compare { dst: r.reg()?, cmp, a: r.reg()?, b: r.reg()? }
        }
        TAG_LOADGLOBAL => Op::LoadGlobal { dst: r.reg()?, slot: r.u32()? },
        TAG_STOREGLOBAL => Op::StoreGlobal { slot: r.u32()?, src: r.reg()? },
        TAG_JUMP => Op::Jump(r.usize()?),
        TAG_JUMPIFFALSE => Op::JumpIfFalse { src: r.reg()?, target: r.usize()? },
        TAG_JUMPIFTRUE => Op::JumpIfTrue { src: r.reg()?, target: r.usize()? },
        TAG_PRINT => Op::Print { src: r.reg()? },
        TAG_HALT => Op::Halt,
        TAG_AWAIT => Op::Await { dst: r.reg()?, src: r.reg()? },
        TAG_CALL => Op::Call {
            dst: r.reg()?,
            base: r.reg()?,
            argc: r.usize()?,
            key: r.str()?,
            shown: r.str()?,
        },
        TAG_RETURN => Op::Return { src: r.reg()? },
        TAG_RETURNUNIT => Op::ReturnUnit,
        TAG_EXT => Op::Ext(X::decode(r)?),
        tag => return Err(WireError::UnknownOpcode(tag)),
    })
}

// ---------------------------------------------------------------------------
// Programs
// ---------------------------------------------------------------------------

impl<X: Wire> Program<X> {
    /// Encodes to bytes, header and all.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        put_u16(&mut out, FORMAT_VERSION);
        put_usize(&mut out, self.frame);
        put_usize(&mut out, self.globals);

        // Sorted, so encoding is deterministic: the same program must produce
        // the same bytes, or a build cannot be compared against a previous one.
        let mut names: Vec<&String> = self.fns.keys().collect();
        names.sort_unstable();
        put_usize(&mut out, names.len());
        for name in names {
            let f = &self.fns[name];
            put_str(&mut out, name);
            put_usize(&mut out, f.addr);
            put_usize(&mut out, f.arity);
            put_usize(&mut out, f.frame);
        }

        put_usize(&mut out, self.code.len());
        for op in &self.code {
            put_op(&mut out, op);
        }
        out
    }

    /// Decodes, checking the header first.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WireError> {
        let mut r = Reader::new(bytes);
        if r.take(4)? != MAGIC {
            return Err(WireError::NotBytecode);
        }
        let version = r.u16()?;
        if version != FORMAT_VERSION {
            return Err(WireError::Version { found: version, expected: FORMAT_VERSION });
        }

        let frame = r.usize()?;
        let globals = r.usize()?;

        let n = r.usize()?;
        let mut fns = HashMap::with_capacity(n);
        for _ in 0..n {
            let name = r.str()?;
            let addr = r.usize()?;
            let arity = r.usize()?;
            let f = r.usize()?;
            fns.insert(name, FnDef { addr, arity, frame: f });
        }

        let n = r.usize()?;
        // Not `with_capacity(n)`: `n` comes from the input, and trusting it
        // lets a four-byte file ask for a gigabyte.
        let mut code = Vec::new();
        for _ in 0..n {
            code.push(get_op(&mut r)?);
        }

        Ok(Program { code, fns, frame, globals })
    }
}
