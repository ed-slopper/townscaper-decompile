//! .NET text formatting and parsing where it reaches files (row E13), as the build's BCL (Mono's, compiled
//! into `GameAssembly.dll`) does it with an en-US / invariant-style culture: `float.ToString("F<n>")` (the OBJ
//! export), `int` / `long` `ToString` and `Parse`, `String.Format` with positional items, and `DateTime`
//! (ticks, `GetHashCode`, custom formats such as `yyyyMMddHHmmss`).
//! Checked against the running game by `tests/unity_dotnet.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------------------------------------
// Mono's NumberFormatter (System.NumberFormatter, called by `Number.FormatSingle` 0xDB7730)

/// `NumberFormatter.DoubleDefPrecision`.
const DOUBLE_DEF_PRECISION: i32 = 15;
/// `NumberFormatter.SingleDefPrecision`.
const SINGLE_DEF_PRECISION: i32 = 7;
/// `NumberFormatter.SeventeenDigitsThreshold`.
const SEVENTEEN_DIGITS_THRESHOLD: i64 = 10_000_000_000_000_000;

/// The exponents where the runtime's `MantissaBitsTable` / `TensExponentTable` (made by Mono's generator, read
/// by the formatter from the runtime) use the next power of ten from the rule in [`mantissa_entry`].
const TENS_BUMPED: [i32; 16] = [2, 198, 291, 394, 487, 683, 776, 879, 972, 1168, 1364, 1457, 1560, 1653, 1849, 2045];

/// Little-endian base-2^32 natural numbers, just what the table needs.
fn big_mul_small(a: &mut Vec<u32>, m: u32) {
    let mut carry = 0u64;
    for x in a.iter_mut() {
        let v = *x as u64 * m as u64 + carry;
        *x = v as u32;
        carry = v >> 32;
    }
    if carry != 0 {
        a.push(carry as u32);
    }
}
fn big_div_small(a: &mut Vec<u32>, d: u32) {
    let mut rem = 0u64;
    for x in a.iter_mut().rev() {
        let v = (rem << 32) | *x as u64;
        *x = (v / d as u64) as u32;
        rem = v % d as u64;
    }
    while a.len() > 1 && *a.last().unwrap() == 0 {
        a.pop();
    }
}
fn big_shr(a: &[u32], n: usize) -> Vec<u32> {
    let (words, bits) = (n / 32, n % 32);
    let mut out: Vec<u32> = vec![];
    for i in words..a.len() {
        let lo = a[i] as u64 >> bits;
        let hi = if bits > 0 && i + 1 < a.len() { (a[i + 1] as u64) << (32 - bits) } else { 0 };
        out.push((lo | hi) as u32);
    }
    if out.is_empty() {
        out.push(0);
    }
    while out.len() > 1 && *out.last().unwrap() == 0 {
        out.pop();
    }
    out
}
fn big_pow2(k: usize) -> Vec<u32> {
    let mut v = vec![0u32; k / 32 + 1];
    v[k / 32] = 1 << (k % 32);
    v
}
/// The value if it fits 64 bits.
fn big_u64(a: &[u32]) -> Option<u64> {
    if a.len() > 2 && a[2..].iter().any(|&x| x != 0) {
        return None;
    }
    Some(a[0] as u64 | (a.get(1).copied().unwrap_or(0) as u64) << 32)
}

/// `floor(2^(e - 1075) / 10^t * 2^64)`, if it fits 64 bits.
fn mantissa_for(e: i32, t: i32) -> Option<u64> {
    let k = e - 1075 + 64;
    let v = if t > 0 {
        if k < 0 {
            return Some(0);
        }
        let mut v = big_pow2(k as usize);
        for _ in 0..t {
            big_div_small(&mut v, 10);
        }
        v
    } else {
        let mut v = vec![1u32];
        for _ in 0..(-t) {
            big_mul_small(&mut v, 10);
        }
        if k >= 0 {
            let mut w = v;
            for _ in 0..k {
                big_mul_small(&mut w, 2);
            }
            w
        } else {
            big_shr(&v, (-k) as usize)
        }
    };
    big_u64(&v)
}

/// `MantissaBitsTable[e]`, `TensExponentTable[e]`: `t` is the least power with `2^(e - 1075) < 10^t` (one more
/// at [`TENS_BUMPED`]), the mantissa `floor(2^(e - 1075) / 10^t * 2^64)`.
pub fn mantissa_entry(e: i32) -> (u64, i32) {
    static CACHE: OnceLock<Mutex<HashMap<i32, (u64, i32)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(&r) = cache.lock().unwrap().get(&e) {
        return r;
    }
    let mut t = ((e - 1075) as f64 * std::f64::consts::LOG10_2).floor() as i32 + 1;
    // 2^(e-1075) < 10^t  <=>  the scaled quotient is below 2^64
    while mantissa_for(e, t).is_none() {
        t += 1;
    }
    while mantissa_for(e, t - 1).is_some() {
        t -= 1;
    }
    if TENS_BUMPED.contains(&e) {
        t += 1;
    }
    let r = (mantissa_for(e, t).unwrap(), t);
    cache.lock().unwrap().insert(e, r);
    r
}

/// The formatter's state after `Init(format, double, defPrecision)`: significant digits (no trailing zeros),
/// the decimal point position, the sign.
#[derive(Clone, Debug, PartialEq)]
struct Digits {
    digits: Vec<u8>,
    dec_point_pos: i32,
    positive: bool,
}

/// `NumberFormatter.InitialFloatingPrecision` for the `F` specifier.
fn initial_floating_precision_fixed(precision: i32, def_precision: i32) -> i32 {
    if precision < def_precision {
        return def_precision;
    }
    def_precision
}

/// `NumberFormatter.Init(string format, double value, int defPrecision)` for a finite value: the mantissa times
/// the table's scaled power of two, normalised to 17 digits (rounded on the dropped 32-bit half), rounded half
/// up to the initial precision.
fn init_double(value: f64, def_precision: i32, initial_precision: i32) -> Digits {
    let bits = value.to_bits() as i64;
    let positive = bits >= 0;
    let bits = bits & i64::MAX;
    if bits == 0 {
        return Digits { digits: vec![], dec_point_pos: 1, positive: true };
    }
    let mut e = (bits >> 52) as i32;
    let mut m = bits & ((1i64 << 52) - 1);
    let mut exp_adjust = 0;
    if e == 0 {
        // a double subnormal (never a float): scaled up so no digits are lost
        e = 1;
        let scale = scale_order(m);
        if scale < DOUBLE_DEF_PRECISION {
            exp_adjust = scale - DOUBLE_DEF_PRECISION;
            m *= ten_power(-exp_adjust);
        }
    } else {
        m = (m + (1i64 << 52)) * 10;
        exp_adjust = -1;
    }
    let (table, tens) = mantissa_entry(e);
    let lo = m as u64 & 0xFFFF_FFFF;
    let hi = m as u64 >> 32;
    let lo2 = table & 0xFFFF_FFFF;
    let hi2 = table >> 32;
    let mut mm = (hi.wrapping_mul(lo2)).wrapping_add(lo.wrapping_mul(hi2)).wrapping_add(lo.wrapping_mul(lo2) >> 32);
    let mut res = hi.wrapping_mul(hi2).wrapping_add(mm >> 32) as i64;
    while res < SEVENTEEN_DIGITS_THRESHOLD {
        mm = (mm & 0xFFFF_FFFF).wrapping_mul(10);
        res = res.wrapping_mul(10).wrapping_add((mm >> 32) as i64);
        exp_adjust -= 1;
    }
    if mm & 0x8000_0000 != 0 {
        res += 1;
    }
    let mut order = DOUBLE_DEF_PRECISION + 2;
    let mut dec_point_pos = tens + exp_adjust + order;
    let _ = def_precision;
    if order > initial_precision {
        let val = ten_power(order - initial_precision);
        res = (res + (val >> 1)) / val;
        order = initial_precision;
    }
    if res >= ten_power(order) {
        order += 1;
        dec_point_pos += 1;
    }
    let s = res.to_string();
    let mut digits: Vec<u8> = s.bytes().map(|b| b - b'0').collect();
    // (`InitDecHexDigits` keeps `order` digits; `CountTrailingZeros` drops the zeros at the end)
    while digits.len() < order as usize {
        digits.insert(0, 0);
    }
    while digits.last() == Some(&0) {
        digits.pop();
    }
    Digits { digits, dec_point_pos, positive }
}

/// `NumberFormatter.ScaleOrder`: the number of decimal digits of `hi`.
fn scale_order(hi: i64) -> i32 {
    let mut n = 18;
    while n >= 0 {
        if hi >= ten_power(n) {
            return n + 1;
        }
        n -= 1;
    }
    1
}

/// `NumberFormatter.GetTenPowerOf`.
fn ten_power(n: i32) -> i64 {
    10i64.pow(n as u32)
}

impl Digits {
    /// `NumberFormatter.RoundBits(shift)` 0xDB6D60: drop `shift` digits, half up on the first dropped one;
    /// nothing left (or more dropped than there are): zero, positive.
    fn round_bits(&mut self, shift: i32) {
        if shift <= 0 {
            return;
        }
        let len = self.digits.len() as i32;
        if shift > len {
            self.digits.clear();
            self.dec_point_pos = 1;
            self.positive = true;
            return;
        }
        let keep = (len - shift) as usize;
        let first = self.digits[keep];
        self.digits.truncate(keep);
        if first >= 5 {
            let mut i = keep;
            loop {
                if i == 0 {
                    self.digits.insert(0, 1);
                    self.dec_point_pos += 1;
                    break;
                }
                i -= 1;
                if self.digits[i] == 9 {
                    self.digits[i] = 0;
                } else {
                    self.digits[i] += 1;
                    break;
                }
            }
        }
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        // rounded away entirely: zero, and positive
        if self.digits.is_empty() {
            self.dec_point_pos = 1;
            self.positive = true;
        }
    }

    /// `NumberFormatter.FormatFixedPoint(precision, nfi)` with `-` and `.`.
    fn format_fixed(&mut self, precision: i32) -> String {
        let shift = self.digits.len() as i32 - self.dec_point_pos - precision;
        self.round_bits(shift);
        let mut out = String::new();
        if !self.positive {
            out.push('-');
        }
        let int_digits = if self.dec_point_pos > 0 { self.dec_point_pos } else { 1 };
        let digit = |i: i32| -> char {
            if i >= 0 && (i as usize) < self.digits.len() {
                (b'0' + self.digits[i as usize]) as char
            } else {
                '0'
            }
        };
        // integer part: the digits before the point (zeros past the significant ones)
        for k in 0..int_digits {
            let i = k - (int_digits - self.dec_point_pos);
            out.push(if self.dec_point_pos > 0 { digit(i) } else { '0' });
        }
        if precision > 0 {
            out.push('.');
            for k in 0..precision {
                out.push(digit(self.dec_point_pos + k));
            }
        }
        out
    }
}

/// `float.ToString("F<precision>", en-US)`: NaN, `Infinity`, `-Infinity`; else rounded to 7 significant digits
/// first (from Mono's 17-digit approximation), then to `precision` decimals, both half up.
pub fn single_to_string_fixed(value: f32, precision: i32) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let ip = initial_floating_precision_fixed(precision, SINGLE_DEF_PRECISION);
    let mut d = init_double(value as f64, SINGLE_DEF_PRECISION, ip);
    d.format_fixed(precision)
}

// ---------------------------------------------------------------------------------------------------------
// Integers

/// `int.ToString()` / `long.ToString()` (the `G` format: `-` and the digits).
pub fn int_to_string(v: i64) -> String {
    v.to_string()
}

/// Why `Parse` throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// `FormatException`.
    Format,
    /// `OverflowException`.
    Overflow,
}

/// `Number.IsWhite`: space and `\t` .. `\r`.
fn is_white(c: char) -> bool {
    c == ' ' || ('\u{9}'..='\u{d}').contains(&c)
}

/// `long.Parse(s)` / `int.Parse(s)` (`NumberStyles.Integer`): optional white space round a `+` / `-` and the
/// decimal digits; `min` / `max` the type's range.
fn parse_integer(s: &str, min: i64, max: i64) -> Result<i64, ParseError> {
    let t = s.trim_matches(is_white);
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(ParseError::Format);
    }
    let mut v: i128 = 0;
    for c in digits.chars() {
        v = v * 10 + (c as u8 - b'0') as i128;
        if v > (max as i128) + 1 {
            return Err(ParseError::Overflow);
        }
    }
    let v = if neg { -v } else { v };
    if v < min as i128 || v > max as i128 {
        return Err(ParseError::Overflow);
    }
    Ok(v as i64)
}

/// `long.Parse(string)`.
pub fn parse_long(s: &str) -> Result<i64, ParseError> {
    parse_integer(s, i64::MIN, i64::MAX)
}

/// `int.Parse(string)`.
pub fn parse_int(s: &str) -> Result<i32, ParseError> {
    parse_integer(s, i32::MIN as i64, i32::MAX as i64).map(|v| v as i32)
}

// ---------------------------------------------------------------------------------------------------------
// String.Format

/// `String.Format(format, args)` with plain items (`{n}`, `{{`, `}}`); alignment and item formats are not used
/// by the game's file names and lines.
pub fn format(fmt: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut it = fmt.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                out.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                out.push('}');
            }
            '{' => {
                let mut n = String::new();
                for d in it.by_ref() {
                    if d == '}' {
                        break;
                    }
                    n.push(d);
                }
                let i: usize = n.trim().parse().expect("String.Format: FormatException");
                out.push_str(args.get(i).expect("String.Format: index out of range"));
            }
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------
// DateTime

/// `DateTime.TicksPerSecond` etc.
const TICKS_PER_MILLISECOND: i64 = 10_000;
const TICKS_PER_SECOND: i64 = TICKS_PER_MILLISECOND * 1000;
const TICKS_PER_MINUTE: i64 = TICKS_PER_SECOND * 60;
const TICKS_PER_HOUR: i64 = TICKS_PER_MINUTE * 60;
const TICKS_PER_DAY: i64 = TICKS_PER_HOUR * 24;
const DAYS_PER_YEAR: i64 = 365;
const DAYS_PER_4_YEARS: i64 = DAYS_PER_YEAR * 4 + 1;
const DAYS_PER_100_YEARS: i64 = DAYS_PER_4_YEARS * 25 - 1;
const DAYS_PER_400_YEARS: i64 = DAYS_PER_100_YEARS * 4 + 1;
const DAYS_TO_MONTH_365: [i64; 13] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365];
const DAYS_TO_MONTH_366: [i64; 13] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366];
/// The ticks of the Unix epoch, 1970-01-01.
pub const UNIX_EPOCH_TICKS: i64 = 621_355_968_000_000_000;

/// `System.DateTime`: the `dateData` word (ticks in the low 62 bits, the kind in the top two).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DateTime {
    pub date_data: u64,
}

/// `DateTimeKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateTimeKind {
    Unspecified = 0,
    Utc = 1,
    Local = 2,
}

impl DateTime {
    const TICKS_MASK: u64 = 0x3FFF_FFFF_FFFF_FFFF;

    /// `new DateTime(long ticks)` (unspecified kind).
    pub fn from_ticks(ticks: i64) -> DateTime {
        assert!((0..=3_155_378_975_999_999_999).contains(&ticks), "DateTime: ArgumentOutOfRangeException");
        DateTime { date_data: ticks as u64 }
    }

    /// Ticks with a kind (what `UtcNow` / `Now` give: the clock as ticks, `Now` in local time).
    pub fn with_kind(ticks: i64, kind: DateTimeKind) -> DateTime {
        DateTime { date_data: ticks as u64 | ((kind as u64) << 62) }
    }

    /// `DateTime.UtcNow` from the system clock as nanoseconds since the Unix epoch (the BCL keeps 100 ns ticks).
    pub fn utc_from_unix_nanos(nanos: i128) -> DateTime {
        DateTime::with_kind((nanos / 100) as i64 + UNIX_EPOCH_TICKS, DateTimeKind::Utc)
    }

    /// `DateTime.Now`: the same clock moved by the local zone's offset from UTC at that moment (the host's
    /// time zone rules, as `TimeZoneInfo.Local` reads them from the system).
    pub fn local_from_unix_nanos(nanos: i128, utc_offset_seconds: i64) -> DateTime {
        DateTime::with_kind((nanos / 100) as i64 + UNIX_EPOCH_TICKS + utc_offset_seconds * TICKS_PER_SECOND, DateTimeKind::Local)
    }

    /// `DateTime.Ticks` (`InternalTicks`).
    pub fn ticks(&self) -> i64 {
        (self.date_data & Self::TICKS_MASK) as i64
    }

    /// `DateTime.GetHashCode`: the ticks' low word xor high word.
    pub fn get_hash_code(&self) -> i32 {
        let t = self.ticks();
        (t as i32) ^ ((t >> 32) as i32)
    }

    /// `DateTime.GetDatePart`: year, month, day of the Gregorian date.
    pub fn date(&self) -> (i32, i32, i32) {
        let mut n = self.ticks() / TICKS_PER_DAY;
        let y400 = n / DAYS_PER_400_YEARS;
        n -= y400 * DAYS_PER_400_YEARS;
        let mut y100 = n / DAYS_PER_100_YEARS;
        if y100 == 4 {
            y100 = 3;
        }
        n -= y100 * DAYS_PER_100_YEARS;
        let y4 = n / DAYS_PER_4_YEARS;
        n -= y4 * DAYS_PER_4_YEARS;
        let mut y1 = n / DAYS_PER_YEAR;
        if y1 == 4 {
            y1 = 3;
        }
        let year = (y400 * 400 + y100 * 100 + y4 * 4 + y1 + 1) as i32;
        n -= y1 * DAYS_PER_YEAR;
        let leap = y1 == 3 && (y4 != 24 || y100 == 3);
        let days = if leap { &DAYS_TO_MONTH_366 } else { &DAYS_TO_MONTH_365 };
        let mut m = (n >> 5) + 1;
        while n >= days[m as usize] {
            m += 1;
        }
        (year, m as i32, (n - days[m as usize - 1] + 1) as i32)
    }

    /// Hour, minute, second, millisecond.
    pub fn time(&self) -> (i32, i32, i32, i32) {
        let t = self.ticks();
        (
            ((t / TICKS_PER_HOUR) % 24) as i32,
            ((t / TICKS_PER_MINUTE) % 60) as i32,
            ((t / TICKS_PER_SECOND) % 60) as i32,
            ((t / TICKS_PER_MILLISECOND) % 1000) as i32,
        )
    }

    /// `DateTime.ToString(format)` for the custom formats the game uses (`DateTimeFormat.FormatCustomized` with
    /// the Gregorian calendar): `y` runs (`yy` two digits, more: padded to the run), `M` / `MM`, `d` / `dd`,
    /// `H` / `HH`, `h` / `hh`, `m` / `mm`, `s` / `ss`; other characters as they are (quotes and `\` escape).
    pub fn to_string_custom(&self, fmt: &str) -> String {
        let (year, month, day) = self.date();
        let (hour, minute, second, _) = self.time();
        let ch: Vec<char> = fmt.chars().collect();
        let mut out = String::new();
        let mut i = 0;
        let pad = |v: i32, n: usize| format!("{v:0n$}");
        while i < ch.len() {
            let c = ch[i];
            let mut n = 1;
            while i + n < ch.len() && ch[i + n] == c {
                n += 1;
            }
            match c {
                'y' => {
                    if n <= 2 {
                        out += &pad(year % 100, n);
                    } else {
                        out += &pad(year, n);
                    }
                }
                'M' => {
                    assert!(n <= 2, "DateTime: month names not ported");
                    out += &pad(month, n);
                }
                'd' => {
                    assert!(n <= 2, "DateTime: day names not ported");
                    out += &pad(day, n);
                }
                'H' => out += &pad(hour, n.min(2)),
                'h' => {
                    let h = hour % 12;
                    out += &pad(if h == 0 { 12 } else { h }, n.min(2));
                }
                'm' => out += &pad(minute, n.min(2)),
                's' => out += &pad(second, n.min(2)),
                '\'' | '"' => {
                    // a quoted literal
                    let q = c;
                    let mut j = i + 1;
                    while j < ch.len() && ch[j] != q {
                        out.push(ch[j]);
                        j += 1;
                    }
                    i = j + 1;
                    continue;
                }
                '\\' => {
                    if let Some(&e) = ch.get(i + 1) {
                        out.push(e);
                    }
                    i += 2;
                    continue;
                }
                'f' | 'F' | 't' | 'z' | 'K' | 'g' | ':' | '/' => panic!("DateTime: format character {c:?} not ported"),
                _ => {
                    for _ in 0..n {
                        out.push(c);
                    }
                }
            }
            i += n;
        }
        out
    }
}
