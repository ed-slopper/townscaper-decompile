//! `Placemaker.TextSaveSystem` and `TextSaveAlphabet` (row G3a): the town as a short text, the string players
//! share (and `SaveData.saveString`). The town is packed into bits, least significant first, six to a letter of
//! a 64-letter alphabet:
//!
//! - a header: the bit widths of positions, x steps and y offsets (5 bits each), the first corner's x and the
//!   lowest y, both moved up by half the position range;
//! - the voxel types used (count in 4 bits, then each type in 4 bits; a voxel stores its index here + 1);
//! - per corner column, in the save's order: whether x moved (1 bit, then the step) except for the first
//!   column, y above the lowest, a ground flag (1 bit: a type-15 voxel at height 0), then from height 1 up one
//!   value per height: 0 for an empty one, the type index + 1, and the all-ones end marker (left out when the
//!   column reaches height 256).
//!
//! Only voxels above the ground pick the types. Reading trims the string of `\r` / `\n`, ignores trailing zero
//! bits, and fails (with the game's log message) on a letter outside the alphabet, too few bits, or a type
//! index out of range.

use crate::placemaker::save_data::{SaveCorner, SaveVoxel};
use crate::unity::dotnet;
use crate::unity::mathf;

/// `TextSaveAlphabet.alphabet` (`.cctor` 0x31CD00): note `…uvxyzw…`.
pub const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvxyzw0123456789_-";
/// `TextSaveAlphabet.baseLength`.
pub const BASE_LENGTH: i32 = 64;
/// `TextSaveSystem.bitCountBitCount`.
pub const BIT_COUNT_BIT_COUNT: i32 = 5;
/// `TextSaveSystem.typeCountBitCount`.
pub const TYPE_COUNT_BIT_COUNT: i32 = 4;

/// `TextSaveSystem.GetBitsPerCharacter` 0x31D270 (`bitsPerCharacter`, set by the `.cctor` 0x31F0F0): the
/// bits a letter of a 64-letter alphabet holds, 6.
pub fn get_bits_per_character() -> i32 {
    let mut n;
    let mut i = 1;
    let mut v: u32 = 2;
    loop {
        n = i;
        v = v.rotate_left(1);
        i += 1;
        if (v as i32) >= BASE_LENGTH + 1 {
            return n;
        }
    }
}

/// `TextSaveSystem.GetBitCount(biggestValue)` 0x31D250: the bits that hold `0..=biggest` (0 for 0 or less).
pub fn get_bit_count(biggest: i32) -> i32 {
    let mut n = 0;
    if 0 < biggest {
        let mut v: u32 = 1;
        loop {
            v = v.rotate_left(1);
            n += 1;
            if (v as i32) > biggest {
                break;
            }
        }
    }
    n
}

/// `TextSaveSystem.bitArray`: the one bit array both directions use (`new BitArray(1)` in the `.cctor`). It only
/// grows, so bits past what a call writes are left from earlier calls; reads are bounded by its whole length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitArray {
    pub bits: Vec<bool>,
}

impl Default for BitArray {
    fn default() -> BitArray {
        BitArray { bits: vec![false] }
    }
}

impl BitArray {
    /// `if (bitArray.Length < n) bitArray.Length = n` (new bits false).
    fn grow(&mut self, n: usize) {
        if self.bits.len() < n {
            self.bits.resize(n, false);
        }
    }
}

/// `TextSaveSystem.AddValue(values, value, bitCount)` 0x31D1F0: the pair appended.
pub fn add_value(values: &mut Vec<i32>, value: i32, bit_count: i32) {
    values.push(value);
    values.push(bit_count);
}

/// `TextSaveSystem.ReadValue(bitArray, ref index, bitCount)` 0x31E000: `bit_count` bits from `index`, least
/// significant first; past the end: logged, the index moved on anyway, -1.
pub fn read_value(bits: &[bool], index: &mut i32, bit_count: i32, log: &mut dyn FnMut(&str)) -> i32 {
    if *index + bit_count <= bits.len() as i32 {
        let (mut v, mut m) = (0i32, 1i32);
        for _ in 0..bit_count {
            if bits[*index as usize] {
                v = v.wrapping_add(m);
            }
            *index += 1;
            m = m.wrapping_mul(2);
        }
        return v;
    }
    log(&format!("Not enough bits {} {} {}", dotnet::int_to_string(*index as i64), dotnet::int_to_string(bit_count as i64), dotnet::int_to_string(bits.len() as i64)));
    *index += bit_count;
    -1
}

/// `TextSaveSystem.SaveToString(corners, voxels)` 0x31E2F0.
pub fn save_to_string(state: &mut BitArray, corners: &[SaveCorner], voxels: &[SaveVoxel], log: &mut dyn FnMut(&str)) -> String {
    // the bounds, the biggest x step and the types above the ground
    let mut first = true;
    let (mut x_first, mut x_last, mut max_step) = (0i32, 0i32, 0i32);
    let (mut y_min, mut y_max) = (i32::MAX, i32::MIN);
    let mut types: u32 = 0;
    let mut vi = 0usize;
    for c in corners {
        if first {
            first = false;
            y_max = c.y;
            y_min = c.y;
            x_first = c.x;
            x_last = c.x;
        } else {
            if c.x != x_last {
                max_step = mathf::max_i32(max_step, c.x.wrapping_sub(x_last));
                x_last = c.x;
            }
            y_min = mathf::min_i32(y_min, c.y);
            y_max = mathf::max_i32(y_max, c.y);
        }
        for _ in 0..c.count.max(0) {
            let v = voxels[vi];
            vi += 1;
            if v.h != 0 {
                types |= 1u32 << (v.t & 31);
            }
        }
    }
    let range = mathf::max_i32(mathf::abs_i32(x_first), mathf::abs_i32(y_min));
    let pos_bits = get_bit_count(range.wrapping_mul(2));
    let y_span = y_max.wrapping_sub(y_min);
    let half = if pos_bits == 0 { 0 } else { 1i32 << ((pos_bits - 1) & 31) };
    let step_bits = get_bit_count(max_step);
    let y_bits = get_bit_count(y_span);
    let mut values = vec![];
    add_value(&mut values, pos_bits, BIT_COUNT_BIT_COUNT);
    add_value(&mut values, step_bits, BIT_COUNT_BIT_COUNT);
    add_value(&mut values, y_bits, BIT_COUNT_BIT_COUNT);
    add_value(&mut values, x_first.wrapping_add(half), pos_bits);
    add_value(&mut values, y_min.wrapping_add(half), pos_bits);
    // the types: type -> index + 1, in type order (a fresh dictionary enumerates in insertion order)
    let mut type_index = [0i32; 16];
    let mut type_count = 0;
    let mut used = vec![];
    for t in 0..15 {
        if types & (1 << t) != 0 {
            type_count += 1;
            type_index[t] = type_count;
            used.push(t as i32);
        }
    }
    add_value(&mut values, type_count, TYPE_COUNT_BIT_COUNT);
    for t in used {
        add_value(&mut values, t, TYPE_COUNT_BIT_COUNT);
    }
    let type_bits = get_bit_count(type_count + 1);
    let end = (1i32 << (type_bits & 31)) - 1;
    // the columns
    let mut first = true;
    let mut x = x_first;
    let mut vi = 0usize;
    for c in corners {
        if !first {
            if c.x == x {
                add_value(&mut values, 0, 1);
            } else {
                let step = c.x.wrapping_sub(x);
                x = c.x;
                add_value(&mut values, 1, 1);
                add_value(&mut values, step, step_bits);
            }
        } else {
            first = false;
        }
        add_value(&mut values, c.y.wrapping_sub(y_min), y_bits);
        if c.count < 1 {
            add_value(&mut values, 0, 1);
            add_value(&mut values, end, type_bits);
            continue;
        }
        let v = voxels[vi];
        let ground = v.h == 0;
        if ground {
            vi += 1;
        }
        add_value(&mut values, ground as i32, 1);
        let mut height = 1i32;
        let mut k = ground as i32;
        while k < c.count {
            let v = voxels[vi];
            vi += 1;
            if height < v.h as i32 {
                let gap = v.h as i32 - height;
                height += gap;
                for _ in 0..gap {
                    add_value(&mut values, 0, type_bits);
                }
            }
            add_value(&mut values, type_index[v.t as usize & 15], type_bits);
            k += 1;
            height += 1;
        }
        if height < 256 {
            add_value(&mut values, end, type_bits);
        }
    }
    if first {
        log("No voxels where there should be voxels");
    }
    // the bits, then the letters
    let total: i32 = values.chunks(2).map(|p| p[1]).sum();
    state.grow(total.max(0) as usize);
    let bits = &mut state.bits;
    let mut w = 0usize;
    for p in values.chunks(2) {
        let mut v = p[0];
        for _ in 0..p[1] {
            bits[w] = v & 1 != 0;
            w += 1;
            v >>= 1;
        }
    }
    let per = get_bits_per_character();
    let chars = mathf::ceil_to_int(total as f32 / per as f32);
    let alphabet: Vec<char> = ALPHABET.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    for _ in 0..chars {
        let (mut v, mut m) = (0usize, 1usize);
        let mut k = 0;
        while k < per && i < total as usize {
            if bits[i] {
                v += m;
            }
            m *= 2;
            k += 1;
            i += 1;
        }
        out.push(alphabet[v]);
    }
    out
}

/// Why `LoadFromString` gives up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// It returned false, logging this.
    Rejected(String),
    /// It threw (a string of only zero bits reads `BitArray.Get(-1)`).
    Exception(&'static str),
}

/// `TextSaveSystem.LoadFromString(saveString, corners, voxels)` 0x31D450: the town appended to the lists.
/// What was appended before a rejection stays (as in the game).
pub fn load_from_string(state: &mut BitArray, text: &str, corners: &mut Vec<SaveCorner>, voxels: &mut Vec<SaveVoxel>, log: &mut dyn FnMut(&str)) -> Result<(), LoadError> {
    let reject = |m: String, log: &mut dyn FnMut(&str)| {
        log(&m);
        Err(LoadError::Rejected(m))
    };
    let text: Vec<char> = text.trim_matches(|c| c == '\r' || c == '\n').chars().collect();
    if text.is_empty() {
        return reject("Character count is zero".into(), log);
    }
    let per = get_bits_per_character();
    let bit_count = per * text.len() as i32;
    if bit_count < 20 {
        return reject("Bit array has no room for bounds or voxels".into(), log);
    }
    state.grow(bit_count as usize);
    let bits = &mut state.bits;
    let mut i = 0usize;
    for &ch in &text {
        let Some(mut v) = ALPHABET.chars().position(|a| a == ch) else {
            return reject(format!("Character not supported: {ch}"), log);
        };
        for _ in 0..per {
            bits[i] = v & 1 != 0;
            i += 1;
            v >>= 1;
        }
    }
    // the last set bit ends the data
    let mut end = bit_count as usize;
    loop {
        if end == 0 {
            return Err(LoadError::Exception("BitArray.Get(-1): ArgumentOutOfRangeException"));
        }
        if bits[end - 1] {
            break;
        }
        end -= 1;
    }
    let bits = &state.bits[..];
    let end = end as i32;
    let mut at = 0i32;
    let pos_bits = read_value(bits, &mut at, BIT_COUNT_BIT_COUNT, log);
    let step_bits = read_value(bits, &mut at, BIT_COUNT_BIT_COUNT, log);
    let y_bits = read_value(bits, &mut at, BIT_COUNT_BIT_COUNT, log);
    if end < at + pos_bits * 2 + 1 {
        return reject("Bit array has no room for voxels".into(), log);
    }
    let half = if pos_bits == 0 { 0 } else { 1i32 << ((pos_bits - 1) & 31) };
    let mut x = read_value(bits, &mut at, pos_bits, log).wrapping_sub(half);
    let y_min = read_value(bits, &mut at, pos_bits, log).wrapping_sub(half);
    if at + TYPE_COUNT_BIT_COUNT <= end {
        let type_count = read_value(bits, &mut at, TYPE_COUNT_BIT_COUNT, log);
        if at + type_count * TYPE_COUNT_BIT_COUNT <= end {
            let types: Vec<i32> = (0..type_count).map(|_| read_value(bits, &mut at, TYPE_COUNT_BIT_COUNT, log)).collect();
            let type_bits = get_bit_count(type_count + 1);
            let end_marker = (1i32 << (type_bits & 31)) - 1;
            let mut first = true;
            while at < end {
                if !first {
                    if read_value(bits, &mut at, 1, log) == 1 {
                        x = x.wrapping_add(read_value(bits, &mut at, step_bits, log));
                    }
                } else {
                    first = false;
                }
                let y = read_value(bits, &mut at, y_bits, log).wrapping_add(y_min);
                let mut count = 0;
                if read_value(bits, &mut at, 1, log) == 1 {
                    voxels.push(SaveVoxel { t: 15, h: 0 });
                    count = 1;
                }
                let mut h: u8 = 1;
                loop {
                    let v = read_value(bits, &mut at, type_bits, log);
                    if v == end_marker {
                        break;
                    }
                    if v != 0 {
                        let v = v - 1;
                        if type_count <= v || v < 0 {
                            let m = dotnet::format(
                                "Invalid voxel value {0}/{1} at {2} {3}",
                                &[&dotnet::int_to_string(v as i64), &dotnet::int_to_string(type_count as i64), &dotnet::int_to_string(h as i64), &format!("int2({}, {})", x, y)],
                            );
                            return reject(m, log);
                        }
                        voxels.push(SaveVoxel { t: types[v as usize] as u8, h });
                        count += 1;
                        if h == 255 {
                            break;
                        }
                    }
                    h = h.wrapping_add(1);
                }
                corners.push(SaveCorner { x, y, count });
            }
            return Ok(());
        }
    }
    reject("Bit array has no room for types".into(), log)
}
