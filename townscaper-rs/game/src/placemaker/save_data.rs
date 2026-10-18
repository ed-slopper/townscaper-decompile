//! `Placemaker.SaveData` and `Placemaker.SettingsData` (row G3): what a town file and the settings file hold.
//! `Graph.Save` writes and `Graph.IterateLoad` reads the town lists (row E4d), the cameras the camera bytes and
//! the thumbnail (G2). The files themselves are [`crate::placemaker::save_xml`].

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

/// `Placemaker.SaveData.Cam`: the camera's tilt (`x`), turn (`y`) and zoom (`z`), each `0..1` as a byte.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SaveCam {
    pub x: u8,
    pub y: u8,
    pub z: u8,
}

/// `Placemaker.SaveData.Sun`: the sun's height and turn, `0..1` as bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SaveSun {
    pub h: u8,
    pub r: u8,
}

/// `Placemaker.SaveData` (`[Serializable]`): a town file. The town as corners with voxel counts and the voxels
/// in corner order, its text form, the write time (UTC ticks), camera and sun, and the thumbnail JPEG.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveData {
    pub save_string: Option<String>,
    pub last_write_time: i64,
    pub cam: SaveCam,
    pub sun: SaveSun,
    pub corners: Vec<SaveCorner>,
    pub voxels: Vec<SaveVoxel>,
    pub big_jpg: Option<Vec<u8>>,
}

impl Default for SaveData {
    /// `SaveData..ctor` 0x967AE0: an empty text, the camera and sun at the middle (`(127, 0, 127)`,
    /// `(127, 127)`), empty lists, no image.
    fn default() -> SaveData {
        SaveData {
            save_string: Some(String::new()),
            last_write_time: 0,
            cam: SaveCam { x: 0x7F, y: 0, z: 0x7F },
            sun: SaveSun { h: 0x7F, r: 0x7F },
            corners: vec![],
            voxels: vec![],
            big_jpg: None,
        }
    }
}

impl SaveData {
    /// `SaveData.Clear` 0x967850: camera and sun back to the middle, the town emptied (text, time and image kept).
    pub fn clear(&mut self) {
        self.cam = SaveCam { x: 0x7F, y: 0, z: 0x7F };
        self.sun = SaveSun { h: 0x7F, r: 0x7F };
        self.corners.clear();
        self.voxels.clear();
    }

    /// `SaveData.ReadNonImageDataFrom(other)` 0x967A10: the town, camera and sun of `other`.
    pub fn read_non_image_data_from(&mut self, other: &SaveData) {
        self.corners.clear();
        self.voxels.clear();
        self.corners.extend_from_slice(&other.corners);
        self.voxels.extend_from_slice(&other.voxels);
        self.cam = other.cam;
        self.sun = other.sun;
    }

    /// `SaveData.CopyFrom(other)` 0x9678D0: as [`Self::read_non_image_data_from`], and the image copied (into a
    /// new array when the lengths differ; nothing when `other` has none).
    pub fn copy_from(&mut self, other: &SaveData) {
        self.read_non_image_data_from(other);
        if let Some(src) = &other.big_jpg {
            match &mut self.big_jpg {
                Some(dst) if dst.len() == src.len() => dst.copy_from_slice(src),
                _ => self.big_jpg = Some(src.clone()),
            }
        }
    }
}

/// `Placemaker.SaveData.C.ToString` 0x4FAD70.
impl std::fmt::Display for SaveCorner {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "x = {}, y = {}, count = {}", self.x, self.y, self.count)
    }
}

/// `Placemaker.SettingsData.Axis`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SettingsAxis {
    pub rotation_x_inverted: bool,
    pub rotation_y_inverted: bool,
}

/// `Placemaker.SettingsData.Screenshot`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SettingsScreenshot {
    pub path: Option<String>,
    pub x: i32,
    pub y: i32,
}

/// `Placemaker.SettingsData.ObjExport`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SettingsObjExport {
    pub path: Option<String>,
}

/// `Placemaker.SettingsData`: the `Sett.ings` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsData {
    pub version: i32,
    pub last_save: Option<String>,
    pub current_color: u8,
    pub audio_volume: u8,
    pub anti_aliasing: bool,
    pub axis: SettingsAxis,
    pub v_sync: bool,
    pub fullscreen: bool,
    pub language: Option<String>,
    pub screenshot: SettingsScreenshot,
    pub obj_export: SettingsObjExport,
    pub ui_size_percentage: u8,
    pub last_run_version: Option<String>,
}

impl Default for SettingsData {
    /// `SettingsData..ctor` 0x973460: full volume, antialiasing and fullscreen on, no language, 2048² shots,
    /// UI at 100%, last run "1.0.0".
    fn default() -> SettingsData {
        SettingsData {
            version: 0,
            last_save: None,
            current_color: 0,
            audio_volume: 0xFF,
            anti_aliasing: true,
            axis: SettingsAxis::default(),
            v_sync: false,
            fullscreen: true,
            language: Some(String::new()),
            screenshot: SettingsScreenshot { path: Some(String::new()), x: 0x800, y: 0x800 },
            obj_export: SettingsObjExport { path: Some(String::new()) },
            ui_size_percentage: 100,
            last_run_version: Some("1.0.0".into()),
        }
    }
}
