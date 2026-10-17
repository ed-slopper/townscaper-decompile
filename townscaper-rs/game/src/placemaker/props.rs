//! Props (row E10): the small objects modules carry — chimneys, lamps, flags, wires, plants. Row E10a: the data:
//! `PropPlacement` (a prop on a module), `Prop` (a prop prefab) and `PropNode` (one prop instance in the graph of
//! props that decides which of them show: parents, children, overlaps, "motevees" and anchors). Checked against
//! the running game by `tests/placemaker_props.rs` (`TS_HARNESS=1`).

use crate::placemaker::modules::PropPlacement;
use crate::unity::quaternion::{Matrix4x4, Quaternion};
use crate::unity::vector::{v3, Vector3};

/// `PropPlacement.byteToAngle`: a rotation byte is 1.40625° (256 to the turn).
pub const BYTE_TO_ANGLE: f32 = 1.40625;
/// `PropPlacement.angleToByte`.
pub const ANGLE_TO_BYTE: f32 = 0.7111111;

impl PropPlacement {
    /// `Placemaker.PropPlacement.get_rotation` 0x707B00: the Euler angles in degrees.
    pub fn rotation(&self) -> Vector3 {
        let r = self.rotation_byte;
        v3(r.x as f32 * BYTE_TO_ANGLE, r.y as f32 * BYTE_TO_ANGLE, r.z as f32 * BYTE_TO_ANGLE)
    }

    /// `Placemaker.PropPlacement.set_rotation` 0x707B50: each angle `(byte)(int)((float)fmod((double)(a + 360f),
    /// 360.0) * angleToByte + 0.5f)` (a full turn wraps to 0 through the byte).
    pub fn set_rotation(&mut self, value: Vector3) {
        let b = |a: f32| {
            let m = (((a + 360.0) as f64) % 360.0) as f32;
            crate::unity::conv::f32_to_i32(m * ANGLE_TO_BYTE + 0.5) as u8
        };
        self.rotation_byte.x = b(value.x);
        self.rotation_byte.y = b(value.y);
        self.rotation_byte.z = b(value.z);
    }

    /// `Placemaker.PropPlacement.GetMatrix` 0x707990: `TRS(pos, Euler(rotation), scale * 2)` (positions in the
    /// qube's -1..1 frame, scales up to 2).
    pub fn get_matrix(&self) -> Matrix4x4 {
        let r = self.rotation();
        let q = Quaternion::euler(r.x, r.y, r.z);
        let s = self.scale.to_vector3();
        Matrix4x4::trs(self.pos.to_vector3(), q, v3(s.x * 2.0, s.y * 2.0, s.z * 2.0))
    }
}

/// `Placemaker.Prop`: a prop prefab (`srcProp` is the prefab an instance was made from).
#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    /// Which voxel types it may stand on (bit per type; `.ctor` 0x70FAE0: all).
    pub voxel_type_mask: u8,
}

impl Default for Prop {
    fn default() -> Self {
        Prop { voxel_type_mask: 0xFF }
    }
}

/// A prop node, as the host numbers them (the original holds references).
pub type NodeId = usize;

/// `System.Single.CompareTo(float)`: NaN sorts first.
pub fn single_compare_to(m: f32, v: f32) -> i32 {
    if m < v {
        -1
    } else if m > v {
        1
    } else if m == v {
        0
    } else if m.is_nan() {
        if v.is_nan() {
            0
        } else {
            -1
        }
    } else {
        1
    }
}

/// `Placemaker.Props.PropNode`. `relations` holds the related nodes in five runs: parents, children, overlaps
/// (sorted by priority), motevees, anchors; the counts mark the runs.
#[derive(Clone, Debug, PartialEq)]
pub struct PropNode {
    pub priority: f32,
    pub appearance_sound: i32,
    pub achievement: u8,
    pub turned_on: bool,
    pub in_dict: bool,
    pub has_been_turned_on: bool,
    pub hide_count: u8,
    pub motivations: u8,
    pub relations: Vec<NodeId>,
    pub parent_count: u8,
    pub child_count: u8,
    pub overlap_count: u8,
    pub motevee_count: u8,
    pub anchor_count: u8,
}

impl Default for PropNode {
    /// `.ctor` 0x7078C0: one motivation, no relations.
    fn default() -> Self {
        PropNode {
            priority: 0.0,
            appearance_sound: 0,
            achievement: 0,
            turned_on: false,
            in_dict: false,
            has_been_turned_on: false,
            hide_count: 0,
            motivations: 1,
            relations: Vec::new(),
            parent_count: 0,
            child_count: 0,
            overlap_count: 0,
            motevee_count: 0,
            anchor_count: 0,
        }
    }
}

impl PropNode {
    /// `get_parentIndex` 0x222F90: 0.
    pub fn parent_index(&self) -> i32 {
        0
    }
    /// `get_childIndex` 0x707940.
    pub fn child_index(&self) -> i32 {
        self.parent_count as i32
    }
    /// `get_overlapIndex` 0x707970.
    pub fn overlap_index(&self) -> i32 {
        self.child_count as i32 + self.parent_count as i32
    }
    /// `get_moteveeIndex` 0x707950.
    pub fn motevee_index(&self) -> i32 {
        self.overlap_count as i32 + self.child_count as i32 + self.parent_count as i32
    }
    /// `get_anchorIndex` 0x707920.
    pub fn anchor_index(&self) -> i32 {
        self.motevee_count as i32 + self.overlap_count as i32 + self.child_count as i32 + self.parent_count as i32
    }

    /// `List.Insert`: false (and nothing changes) where the original throws `ArgumentOutOfRangeException`: past
    /// the end, which the counts can reach once a `Remove…` has lowered the wrong one (a count wraps at 0).
    fn insert(&mut self, at: i32, node: NodeId) -> bool {
        if at < 0 || at as usize > self.relations.len() {
            return false;
        }
        self.relations.insert(at as usize, node);
        true
    }

    /// `AddParent` 0x7075E0: at the end of the parents.
    pub fn add_parent(&mut self, node: NodeId) -> bool {
        if !self.insert(self.parent_count as i32, node) {
            return false;
        }
        self.parent_count = self.parent_count.wrapping_add(1);
        true
    }
    /// `AddChild` 0x7074C0.
    pub fn add_child(&mut self, node: NodeId) -> bool {
        if !self.insert(self.parent_count as i32 + self.child_count as i32, node) {
            return false;
        }
        self.child_count = self.child_count.wrapping_add(1);
        true
    }
    /// `AddOverlap` 0x707580: at the end of the overlaps.
    pub fn add_overlap(&mut self, node: NodeId) -> bool {
        if !self.insert(self.overlap_index() + self.overlap_count as i32, node) {
            return false;
        }
        self.overlap_count = self.overlap_count.wrapping_add(1);
        true
    }
    /// `AddMotevee` 0x707520.
    pub fn add_motevee(&mut self, node: NodeId) -> bool {
        if !self.insert(self.motevee_index() + self.motevee_count as i32, node) {
            return false;
        }
        self.motevee_count = self.motevee_count.wrapping_add(1);
        true
    }
    /// `AddAnchor` 0x707450.
    pub fn add_anchor(&mut self, node: NodeId) -> bool {
        if !self.insert(self.anchor_index() + self.anchor_count as i32, node) {
            return false;
        }
        self.anchor_count = self.anchor_count.wrapping_add(1);
        true
    }

    /// `InserOverlap` 0x707630: into the overlaps where `List.BinarySearch(overlapIndex, overlapCount, node, this)`
    /// puts it (the comparer is the node's own `Compare`: by `priority`, `Single.CompareTo`).
    pub fn inser_overlap(&mut self, node: NodeId, priority: &dyn Fn(NodeId) -> f32) -> bool {
        // `List.BinarySearch(index, count, ..)` checks its range first (`ArgumentException`)
        let (index, count) = (self.overlap_index(), self.overlap_count as i32);
        if (self.relations.len() as i32) - index < count {
            return false;
        }
        let mut lo = index;
        let mut hi = lo + count - 1;
        let p = priority(node);
        let at = loop {
            if lo > hi {
                break lo;
            }
            let mid = lo + ((hi - lo) >> 1);
            // (with wrapped counts the original reads past the end and throws)
            let Some(&r) = self.relations.get(mid as usize) else { return false };
            let c = single_compare_to(priority(r), p);
            if c == 0 {
                break mid;
            }
            if c < 0 {
                lo = mid + 1;
            } else {
                hi = mid - 1;
            }
        };
        if !self.insert(at, node) {
            return false;
        }
        self.overlap_count = self.overlap_count.wrapping_add(1);
        true
    }

    /// `List.Remove`: the first occurrence anywhere in `relations`. The `Remove…` methods all use it and then
    /// lower their own count (whichever run the node was in: the original does the same).
    fn remove(&mut self, node: NodeId) -> bool {
        match self.relations.iter().position(|&n| n == node) {
            Some(i) => {
                self.relations.remove(i);
                true
            }
            None => false,
        }
    }
    /// `RemoveParent` 0x707800.
    pub fn remove_parent(&mut self, node: NodeId) {
        if self.remove(node) {
            self.parent_count = self.parent_count.wrapping_sub(1);
        }
    }
    /// `RemoveChild` 0x707710.
    pub fn remove_child(&mut self, node: NodeId) {
        if self.remove(node) {
            self.child_count = self.child_count.wrapping_sub(1);
        }
    }
    /// `RemoveOverlap` 0x7077B0.
    pub fn remove_overlap(&mut self, node: NodeId) {
        if self.remove(node) {
            self.overlap_count = self.overlap_count.wrapping_sub(1);
        }
    }
    /// `RemoveMotevee` 0x707760.
    pub fn remove_motevee(&mut self, node: NodeId) {
        if self.remove(node) {
            self.motevee_count = self.motevee_count.wrapping_sub(1);
        }
    }
    /// `RemoveAnchor` 0x7076C0.
    pub fn remove_anchor(&mut self, node: NodeId) {
        if self.remove(node) {
            self.anchor_count = self.anchor_count.wrapping_sub(1);
        }
    }

    /// `IComparable<PropNode>.CompareTo` 0x707890 and `IComparer<PropNode>.Compare` 0x707850: by `priority`.
    pub fn compare(a: &PropNode, b: &PropNode) -> i32 {
        single_compare_to(a.priority, b.priority)
    }
}
