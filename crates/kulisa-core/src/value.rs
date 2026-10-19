//! Plain data for saves, rollback and story variables. Never live objects or code.

use std::collections::BTreeMap;

/// A self-describing value tree. Maps are string-keyed and ordered, so encoding is
/// deterministic.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn map() -> Self {
        Value::Map(BTreeMap::new())
    }

    /// Builder-style insert for maps; no-op on other variants.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        if let Value::Map(m) = &mut self {
            m.insert(key.to_string(), value.into());
        }
        self
    }

    /// Field of a map; `Nil` when missing or not a map (tolerant reads for old saves).
    pub fn get(&self, key: &str) -> &Value {
        static NIL: Value = Value::Nil;
        match self {
            Value::Map(m) => m.get(key).unwrap_or(&NIL),
            _ => &NIL,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        if let Value::Str(s) = self { Some(s) } else { None }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }

    pub fn as_list(&self) -> &[Value] {
        if let Value::List(l) = self { l } else { &[] }
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<u32> for Value {
    fn from(v: u32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<usize> for Value {
    fn from(v: usize) -> Self {
        Value::Int(v as i64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::List(v)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(v: Option<T>) -> Self {
        v.map_or(Value::Nil, Into::into)
    }
}

// ---- Binary encoding: one tag byte per value, LEB128 lengths, zigzag ints. ----

const NIL: u8 = 0;
const FALSE: u8 = 1;
const TRUE: u8 = 2;
const INT: u8 = 3;
const FLOAT: u8 = 4;
const STR: u8 = 5;
const BYTES: u8 = 6;
const LIST: u8 = 7;
const MAP: u8 = 8;
/// Nesting limit when decoding untrusted files.
const MAX_DEPTH: usize = 128;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let byte = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

impl Value {
    pub fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Value::Nil => out.push(NIL),
            Value::Bool(false) => out.push(FALSE),
            Value::Bool(true) => out.push(TRUE),
            Value::Int(i) => {
                out.push(INT);
                put_varint(out, ((i << 1) ^ (i >> 63)) as u64);
            }
            Value::Float(f) => {
                out.push(FLOAT);
                out.extend_from_slice(&f.to_le_bytes());
            }
            Value::Str(s) => {
                out.push(STR);
                put_varint(out, s.len() as u64);
                out.extend_from_slice(s.as_bytes());
            }
            Value::Bytes(b) => {
                out.push(BYTES);
                put_varint(out, b.len() as u64);
                out.extend_from_slice(b);
            }
            Value::List(l) => {
                out.push(LIST);
                put_varint(out, l.len() as u64);
                l.iter().for_each(|v| v.encode(out));
            }
            Value::Map(m) => {
                out.push(MAP);
                put_varint(out, m.len() as u64);
                for (k, v) in m {
                    put_varint(out, k.len() as u64);
                    out.extend_from_slice(k.as_bytes());
                    v.encode(out);
                }
            }
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode(&mut out);
        out
    }

    /// Decodes one value from the front of `input`, advancing it.
    pub fn decode(input: &mut &[u8]) -> Result<Value, String> {
        decode(input, 0)
    }
}

fn take<'a>(input: &mut &'a [u8], n: usize) -> Result<&'a [u8], String> {
    if input.len() < n {
        return Err("unexpected end of data".into());
    }
    let (head, rest) = input.split_at(n);
    *input = rest;
    Ok(head)
}

fn varint(input: &mut &[u8]) -> Result<u64, String> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = take(input, 1)?[0];
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Err("varint too long".into())
}

fn length(input: &mut &[u8]) -> Result<usize, String> {
    let n = varint(input)? as usize;
    // A length can never exceed the remaining bytes (each element takes at least one).
    if n > input.len() {
        return Err("length exceeds data".into());
    }
    Ok(n)
}

fn string(input: &mut &[u8]) -> Result<String, String> {
    let n = length(input)?;
    String::from_utf8(take(input, n)?.to_vec()).map_err(|_| "invalid UTF-8".to_string())
}

fn decode(input: &mut &[u8], depth: usize) -> Result<Value, String> {
    if depth > MAX_DEPTH {
        return Err("value nested too deeply".into());
    }
    Ok(match take(input, 1)?[0] {
        NIL => Value::Nil,
        FALSE => Value::Bool(false),
        TRUE => Value::Bool(true),
        INT => {
            let z = varint(input)?;
            Value::Int(((z >> 1) as i64) ^ -((z & 1) as i64))
        }
        FLOAT => Value::Float(f64::from_le_bytes(take(input, 8)?.try_into().unwrap())),
        STR => Value::Str(string(input)?),
        BYTES => {
            let n = length(input)?;
            Value::Bytes(take(input, n)?.to_vec())
        }
        LIST => {
            let n = length(input)?;
            let mut l = Vec::with_capacity(n);
            for _ in 0..n {
                l.push(decode(input, depth + 1)?);
            }
            Value::List(l)
        }
        MAP => {
            let n = length(input)?;
            let mut m = BTreeMap::new();
            for _ in 0..n {
                let k = string(input)?;
                m.insert(k, decode(input, depth + 1)?);
            }
            Value::Map(m)
        }
        t => return Err(format!("unknown value tag {t}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(v: Value) {
        let bytes = v.to_bytes();
        let mut input = bytes.as_slice();
        assert_eq!(Value::decode(&mut input).unwrap(), v);
        assert!(input.is_empty());
    }

    #[test]
    fn roundtrips_every_kind() {
        for i in [0i64, 1, -1, 63, -64, 1 << 40, i64::MIN, i64::MAX] {
            roundtrip(Value::Int(i));
        }
        roundtrip(Value::Float(-1.5));
        roundtrip(Value::Str("ćevapi 漢字".into()));
        roundtrip(Value::Bytes(vec![0, 255, 7]));
        roundtrip(
            Value::map()
                .with("trust", 3i64)
                .with("name", "Mila")
                .with("flags", Value::List(vec![true.into(), Value::Nil]))
                .with("nested", Value::map().with("x", 1.25)),
        );
    }

    #[test]
    fn rejects_corrupt_data() {
        assert!(Value::decode(&mut &[STR, 10, b'a'][..]).is_err(), "truncated");
        assert!(Value::decode(&mut &[99][..]).is_err(), "unknown tag");
        assert!(Value::decode(&mut &[LIST, 0xff, 0xff, 0xff, 0x0f][..]).is_err(), "absurd length");
        let deep: Vec<u8> = std::iter::repeat_n([LIST, 1], 200).flatten().chain([NIL]).collect();
        assert!(Value::decode(&mut deep.as_slice()).is_err(), "depth limit");
    }

    #[test]
    fn tolerant_reads() {
        let v = Value::map().with("a", 1i64);
        assert_eq!(v.get("missing"), &Value::Nil);
        assert_eq!(v.get("a").as_int(), Some(1));
        assert_eq!(Value::Float(2.0).as_int(), Some(2));
        assert_eq!(Value::Nil.as_list(), &[] as &[Value]);
    }
}
