//! Process-private binary staging. Field order is shared by both directions, and strings can span I/O buffers.
use crate::model::{
    Direction, FactRecord, GateRecord, IdentityQuality, ObjectKind, StructuralLocator, WordRecord,
};
use std::io::{self, Read, Write};

pub(super) trait Record: Sized {
    fn encode(&self, out: &mut impl Write) -> io::Result<()>;
    fn decode(input: &mut impl Read) -> io::Result<Self>;
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid binary stats record")
}

fn byte(input: &mut impl Read) -> io::Result<u8> {
    let mut b = [0];
    input.read_exact(&mut b)?;
    Ok(b[0])
}

impl Record for u64 {
    fn encode(&self, out: &mut impl Write) -> io::Result<()> {
        let n = *self;
        match n {
            0..=252 => out.write_all(&[n as u8]),
            253..=65535 => {
                out.write_all(&[253])?;
                out.write_all(&(n as u16).to_le_bytes())
            }
            65536..=0xffff_ffff => {
                out.write_all(&[254])?;
                out.write_all(&(n as u32).to_le_bytes())
            }
            _ => {
                out.write_all(&[255])?;
                out.write_all(&n.to_le_bytes())
            }
        }
    }

    fn decode(input: &mut impl Read) -> io::Result<Self> {
        let size = match byte(input)? {
            253 => 2,
            254 => 4,
            255 => 8,
            n => return Ok(n.into()),
        };
        let mut bytes = [0; 8];
        input.read_exact(&mut bytes[..size])?;
        Ok(Self::from_le_bytes(bytes))
    }
}

impl Record for String {
    fn encode(&self, out: &mut impl Write) -> io::Result<()> {
        (self.len() as u64).encode(out)?;
        out.write_all(self.as_bytes())
    }

    fn decode(input: &mut impl Read) -> io::Result<Self> {
        let len = u64::decode(input)?;
        if len <= 8192 {
            let mut bytes = vec![0; len as usize];
            input.read_exact(&mut bytes)?;
            return Self::from_utf8(bytes).map_err(|_| invalid());
        }
        let mut bytes = Vec::new();
        let read = input.take(len).read_to_end(&mut bytes)?;
        if read as u64 != len {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Self::from_utf8(bytes).map_err(|_| invalid())
    }
}

impl Record for bool {
    fn encode(&self, out: &mut impl Write) -> io::Result<()> {
        out.write_all(&[u8::from(*self)])
    }

    fn decode(input: &mut impl Read) -> io::Result<Self> {
        match byte(input)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid()),
        }
    }
}

impl<T: Record> Record for Option<T> {
    fn encode(&self, out: &mut impl Write) -> io::Result<()> {
        self.is_some().encode(out)?;
        if let Some(item) = self {
            item.encode(out)?;
        }
        Ok(())
    }

    fn decode(input: &mut impl Read) -> io::Result<Self> {
        bool::decode(input)?.then(|| T::decode(input)).transpose()
    }
}

impl<T: Record> Record for Vec<T> {
    fn encode(&self, out: &mut impl Write) -> io::Result<()> {
        (self.len() as u64).encode(out)?;
        for item in self {
            item.encode(out)?;
        }
        Ok(())
    }

    fn decode(input: &mut impl Read) -> io::Result<Self> {
        let count = u64::decode(input)?;
        let mut items = Vec::with_capacity(count.min(256) as usize);
        for _ in 0..count {
            items.push(T::decode(input)?);
        }
        Ok(items)
    }
}

macro_rules! record_enum {
    ($ty:ty, $($variant:ident),+) => {
        impl Record for $ty {
            fn encode(&self, out: &mut impl Write) -> io::Result<()> {
                let tag = match self { $(Self::$variant => Self::$variant as u8),+ };
                out.write_all(&[tag])
            }
            fn decode(input: &mut impl Read) -> io::Result<Self> {
                match byte(input)? {
                    $(tag if tag == Self::$variant as u8 => Ok(Self::$variant),)+
                    _ => Err(invalid()),
                }
            }
        }
    };
}

macro_rules! record_struct {
    ($ty:ty, $($field:ident),+) => {
        impl Record for $ty {
            fn encode(&self, out: &mut impl Write) -> io::Result<()> {
                let Self { $($field),+ } = self;
                $($field.encode(out)?;)+
                Ok(())
            }
            fn decode(input: &mut impl Read) -> io::Result<Self> {
                Ok(Self { $($field: Record::decode(input)?),+ })
            }
        }
    };
}

record_enum!(
    ObjectKind,
    MorphRule,
    PhonRule,
    LexEntry,
    RootIndex,
    Guesser,
    Overlay,
    Parser,
    Unattributed
);
record_enum!(Direction, Analysis, Synthesis, Unattributed);
record_enum!(IdentityQuality, Authored, Structural, Synthetic);
record_struct!(StructuralLocator, key, label);
record_struct!(
    FactRecord,
    object_key,
    object_kind,
    object_label,
    identity_quality,
    stratum,
    allomorph,
    morpheme,
    direction,
    attempts,
    work,
    work_steps,
    outputs,
    not_applied,
    no_root,
    surface_mismatch,
    uses,
    self_time_ns
);
record_struct!(
    GateRecord,
    object_key,
    object_kind,
    object_label,
    identity_quality,
    morpheme,
    feature_key,
    feature_kind,
    feature_label,
    mode,
    passed,
    blocked
);
record_struct!(
    WordRecord,
    form,
    elapsed_ns,
    work,
    passes,
    capped,
    timed_out,
    invalid_shape,
    facts,
    gates
);
