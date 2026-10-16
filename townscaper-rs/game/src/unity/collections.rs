//! .NET collection behaviour the game's results depend on (row F11e), as the build's BCL (Mono's copy of the
//! .NET reference source) runs it: `List<T>.Sort` / `Array.Sort` (introsort, not stable), `BinarySearch`, and
//! the iteration order of `Dictionary<K, V>` / `HashSet<T>` (slot order, removed slots reused last-in first-out).
//! Checked against the running game by `tests/unity_collections.rs` (`TS_HARNESS=1`).

use std::collections::HashMap;
use std::hash::Hash;

/// `IntrospectiveSortUtilities.FloorLog2` 0x92E2D0: despite the name, the bit length of `n` (0 for n <= 0).
pub fn floor_log2(n: i32) -> i32 {
    if n <= 0 {
        0
    } else {
        32 - (n as u32).leading_zeros() as i32
    }
}

/// `ArraySortHelper<T>.IntrospectiveSort` 0x7AF9D0 (via `Sort`, `List<T>.Sort`): sorts `keys[index..index +
/// length]` with `cmp` (a C# `Comparison<T>`: negative, zero, positive). `array_len` is the length of the
/// backing array (a `List<T>`'s capacity, not its count): the depth limit is `2 * FloorLog2(array_len)`.
/// Returns true when the heapsort fallback ran (tests use it to cover that path).
pub fn sort_by<T: Clone>(
    keys: &mut [T],
    index: usize,
    length: usize,
    array_len: usize,
    cmp: &mut impl FnMut(&T, &T) -> i32,
) -> bool {
    if length < 2 {
        return false;
    }
    let mut heap = false;
    intro_sort(keys, index as i32, (index + length) as i32 - 1, 2 * floor_log2(array_len as i32), cmp, &mut heap);
    heap
}

/// `ArraySortHelper<T>.SwapIfGreater`.
fn swap_if_greater<T>(k: &mut [T], cmp: &mut impl FnMut(&T, &T) -> i32, a: i32, b: i32) {
    if a != b && cmp(&k[a as usize], &k[b as usize]) > 0 {
        k.swap(a as usize, b as usize);
    }
}

/// `ArraySortHelper<T>.IntroSort` 0x7AF7E0.
fn intro_sort<T: Clone>(
    k: &mut [T],
    lo: i32,
    mut hi: i32,
    mut depth: i32,
    cmp: &mut impl FnMut(&T, &T) -> i32,
    heap: &mut bool,
) {
    while hi > lo {
        let size = hi - lo + 1;
        if size <= 16 {
            match size {
                1 => {}
                2 => swap_if_greater(k, cmp, lo, hi),
                3 => {
                    swap_if_greater(k, cmp, lo, hi - 1);
                    swap_if_greater(k, cmp, lo, hi);
                    swap_if_greater(k, cmp, hi - 1, hi);
                }
                _ => insertion_sort(k, lo, hi, cmp),
            }
            return;
        }
        if depth == 0 {
            *heap = true;
            heapsort(k, lo, hi, cmp);
            return;
        }
        depth -= 1;
        let p = pick_pivot_and_partition(k, lo, hi, cmp);
        intro_sort(k, p + 1, hi, depth, cmp, heap);
        hi = p - 1;
    }
}

/// `ArraySortHelper<T>.PickPivotAndPartition` 0xD712A0: median of three, pivot parked at `hi - 1`.
fn pick_pivot_and_partition<T: Clone>(k: &mut [T], lo: i32, hi: i32, cmp: &mut impl FnMut(&T, &T) -> i32) -> i32 {
    let mid = lo + (hi - lo) / 2;
    swap_if_greater(k, cmp, lo, mid);
    swap_if_greater(k, cmp, lo, hi);
    swap_if_greater(k, cmp, mid, hi);
    let pivot = k[mid as usize].clone();
    k.swap(mid as usize, (hi - 1) as usize);
    let (mut left, mut right) = (lo, hi - 1);
    while left < right {
        loop {
            left += 1;
            if cmp(&k[left as usize], &pivot) >= 0 {
                break;
            }
        }
        loop {
            right -= 1;
            if cmp(&pivot, &k[right as usize]) >= 0 {
                break;
            }
        }
        if left >= right {
            break;
        }
        k.swap(left as usize, right as usize);
    }
    k.swap(left as usize, (hi - 1) as usize);
    left
}

/// `ArraySortHelper<T>.InsertionSort` 0xD6E600.
fn insertion_sort<T: Clone>(k: &mut [T], lo: i32, hi: i32, cmp: &mut impl FnMut(&T, &T) -> i32) {
    for i in lo..hi {
        let mut j = i;
        let t = k[(i + 1) as usize].clone();
        while j >= lo && cmp(&t, &k[j as usize]) < 0 {
            k[(j + 1) as usize] = k[j as usize].clone();
            j -= 1;
        }
        k[(j + 1) as usize] = t;
    }
}

/// `ArraySortHelper<T>.Heapsort` 0x7AEEE0.
fn heapsort<T: Clone>(k: &mut [T], lo: i32, hi: i32, cmp: &mut impl FnMut(&T, &T) -> i32) {
    let n = hi - lo + 1;
    let mut i = n / 2;
    while i >= 1 {
        down_heap(k, i, n, lo, cmp);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        k.swap(lo as usize, (lo + i - 1) as usize);
        down_heap(k, 1, i - 1, lo, cmp);
        i -= 1;
    }
}

/// `ArraySortHelper<T>.DownHeap` 0xD6D1B0.
fn down_heap<T: Clone>(k: &mut [T], mut i: i32, n: i32, lo: i32, cmp: &mut impl FnMut(&T, &T) -> i32) {
    let d = k[(lo + i - 1) as usize].clone();
    while i <= n / 2 {
        let mut child = 2 * i;
        if child < n && cmp(&k[(lo + child - 1) as usize], &k[(lo + child) as usize]) < 0 {
            child += 1;
        }
        if cmp(&d, &k[(lo + child - 1) as usize]) >= 0 {
            break;
        }
        k[(lo + i - 1) as usize] = k[(lo + child - 1) as usize].clone();
        i = child;
    }
    k[(lo + i - 1) as usize] = d;
}

/// `ArraySortHelper<T>.InternalBinarySearch` 0xD6F270 (`List<T>.BinarySearch`, `Array.BinarySearch`): the index
/// of a match (whichever the halving reaches first), else the bitwise complement of the insertion point.
pub fn binary_search<T>(a: &[T], index: usize, length: usize, value: &T, cmp: &mut impl FnMut(&T, &T) -> i32) -> i32 {
    let mut lo = index as i32;
    let mut hi = (index + length) as i32 - 1;
    while lo <= hi {
        let i = lo + ((hi - lo) >> 1);
        let order = cmp(&a[i as usize], value);
        if order == 0 {
            return i;
        }
        if order < 0 {
            lo = i + 1;
        } else {
            hi = i - 1;
        }
    }
    !lo
}

/// `List<T>`'s backing-array growth: past the capacity it doubles (4 from empty), at least to what is needed.
/// Only the capacity matters to the port (it is the sort's `array_len`).
pub fn list_grow(capacity: usize, needed: usize) -> usize {
    if needed <= capacity {
        return capacity;
    }
    let doubled = if capacity == 0 { 4 } else { capacity * 2 };
    doubled.max(needed)
}

/// `Dictionary<K, V>` and `HashSet<T>` (value `()`) with the BCL's iteration order: entries live in slots; a
/// new key takes the most recently freed slot, else the next unused one; enumeration walks the slots.
#[derive(Clone, Debug)]
pub struct SlotMap<K, V> {
    slots: Vec<Option<(K, V)>>,
    free: Vec<usize>,
    index: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, V> Default for SlotMap<K, V> {
    fn default() -> Self {
        SlotMap { slots: Vec::new(), free: Vec::new(), index: HashMap::new() }
    }
}

impl<K: Eq + Hash + Clone, V> SlotMap<K, V> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn len(&self) -> usize {
        self.index.len()
    }
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
    pub fn contains_key(&self, k: &K) -> bool {
        self.index.contains_key(k)
    }
    pub fn get(&self, k: &K) -> Option<&V> {
        self.index.get(k).map(|&i| &self.slots[i].as_ref().unwrap().1)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let i = *self.index.get(k)?;
        Some(&mut self.slots[i].as_mut().unwrap().1)
    }
    /// `Dictionary.Add` (which throws on a duplicate; here: false, no change) / `HashSet.Add`.
    pub fn add(&mut self, k: K, v: V) -> bool {
        if self.index.contains_key(&k) {
            return false;
        }
        let i = match self.free.pop() {
            Some(i) => i,
            None => {
                self.slots.push(None);
                self.slots.len() - 1
            }
        };
        self.slots[i] = Some((k.clone(), v));
        self.index.insert(k, i);
        true
    }
    /// The indexer's set: replaces in place, or adds.
    pub fn set(&mut self, k: K, v: V) {
        match self.index.get(&k) {
            Some(&i) => self.slots[i].as_mut().unwrap().1 = v,
            None => {
                self.add(k, v);
            }
        }
    }
    pub fn remove(&mut self, k: &K) -> Option<V> {
        let i = self.index.remove(k)?;
        self.free.push(i);
        self.slots[i].take().map(|(_, v)| v)
    }
    /// `Clear`: every slot unused, no free list.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.index.clear();
    }
    /// Enumeration order (`foreach`, `Keys`, `Values`, `CopyTo`).
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.slots.iter().flatten().map(|(k, v)| (k, v))
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(k, _)| k)
    }
}
