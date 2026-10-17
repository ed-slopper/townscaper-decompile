//! `Placemaker.SaveData` (row G3): so far the town itself, the corner and voxel lists `Graph.Save` writes and
//! `Graph.IterateLoad` reads (row E4d). The camera, sun, thumbnail and the file side come with G3.

/// `Placemaker.SaveData.C`: a corner with voxels, and how many of the voxel list are its.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SaveCorner {
    pub x: i32,
    pub y: i32,
    pub count: i32,
}

/// `Placemaker.SaveData.V`: one voxel, its type and height.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SaveVoxel {
    pub t: u8,
    pub h: u8,
}

/// `Placemaker.SaveData`: the town part.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SaveData {
    pub corners: Vec<SaveCorner>,
    pub voxels: Vec<SaveVoxel>,
}
