//! `Placemaker.SaveSystem` and `Placemaker.MetaSave` (row G3b): the towns on disk and the settings file.
//!
//! Each town file has a `MetaSave` (the save menu's card shows it): its path, state flags, last write time and
//! save data. Live ones sit in `metaSaveContainer` in the menu's order (newest first, the current town on top);
//! spare ones in `metaSavePool` (child 0 is the template that new ones are copied from). The system boots by
//! loading `Sett.ings` (or making new settings) and the last town, then refreshes: lists the storage
//! container's files, drops metas whose file went away, reloads the rest, adds metas for new files, sorts.
//! Writing goes through the storage container as `XmlSerializer` bytes (`.binscape`: `BinaryFormatter`).
//!
//! The Unity side (the meta objects' game objects and transforms, their textures, the cards' callbacks) and the
//! world master are the host's ([`SaveSystemHost`]); the storage container (the Fury SDK's, middleware) is
//! [`StorageContainer`], with its requests completing through [`SaveSystem::complete`].

use crate::placemaker::save_data::{SaveData, SettingsData};
use crate::placemaker::save_xml;
use crate::unity::mathematics::Random;

/// `MetaSave.State` (flags).
pub mod meta_state {
    pub const UNCLEAR: u32 = 1;
    pub const NEEDS_UPDATING: u32 = 2;
    pub const UP_TO_DATE: u32 = 4;
    pub const NOT_YET_SAVED: u32 = 8;
    pub const NEW_FROM_DUPLICATE: u32 = 16;
    pub const FAILED_TO_LOAD: u32 = 32;
    pub const DID_NOT_EXIST_ON_DISK: u32 = 64;
    pub const DELETED: u32 = 128;
    pub const RETURNED_TO_POOL: u32 = 256;
    pub const BROKEN_MASK: u32 = 480;
    pub const INVISIBLE_MASK: u32 = 483;
    pub const SHOULD_BE_IN_POOL_MASK: u32 = 448;
}
use meta_state as ms;

/// `SaveSystem.settingsFileName`.
pub const SETTINGS_FILE_NAME: &str = "Sett.ings";
/// `SaveSystem.saveFilePrefix`.
pub const SAVE_FILE_PREFIX: &str = "Town";
/// `SaveSystem.xmlExtension`.
pub const XML_EXTENSION: &str = ".scape";
/// `SaveSystem.binExtension`.
pub const BIN_EXTENSION: &str = ".binscape";
/// `SaveSystem.saveFileDirectoryName`.
pub const SAVE_FILE_DIRECTORY_NAME: &str = "Saves";
/// `SaveSystem.containerID`.
pub const CONTAINER_ID: &str = "storage";
/// `SaveSystem.saveChars` (`..ctor` 0x96E320): note `uvxyzw`.
pub const SAVE_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvxyzw0123456789";
/// `SaveSystem.charArray`'s length: the random part of a file name.
pub const SAVE_NAME_CHARS: usize = 16;
/// The path given to metas that have none yet.
pub const NO_PATH: &str = "no path";
/// `Path.DirectorySeparatorChar` on Windows.
pub const DIRECTORY_SEPARATOR: char = '\\';

/// A meta save's index in [`SaveSystem::metas`].
pub type MetaSaveId = usize;
/// A storage request (the host's).
pub type RequestId = u64;

/// `Placemaker.MetaSave`: one town file.
#[derive(Clone, Debug, PartialEq)]
pub struct MetaSave {
    pub path: String,
    pub file_name: String,
    pub is_bin: bool,
    /// The card's `Texture2D` (the host's).
    pub texture: u64,
    pub last_file_write_time: i64,
    pub save_data: Option<SaveData>,
    pub last_disk_sync_time: f32,
    pub last_texture_load_time: f32,
    pub last_save_data_time: f32,
    pub exists_on_disk: bool,
    pub load_request: Option<RequestId>,
    pub save_request: Option<RequestId>,
    pub state: u32,
    /// `Object.GetInstanceID` (the last tie-break of the order).
    pub instance_id: i32,
    /// The game object's `activeSelf`.
    pub active: bool,
}

impl MetaSave {
    /// The pool's template as the scene has it: state unclear, no path, no times.
    pub fn template(instance_id: i32) -> MetaSave {
        MetaSave {
            path: String::new(),
            file_name: String::new(),
            is_bin: false,
            texture: 0,
            last_file_write_time: 0,
            save_data: Some(SaveData::default()),
            last_disk_sync_time: -1.0,
            last_texture_load_time: -1.0,
            last_save_data_time: -1.0,
            exists_on_disk: false,
            load_request: None,
            save_request: None,
            state: ms::UNCLEAR,
            instance_id,
            active: true,
        }
    }
    /// `MetaSave.get_isXml` 0x32D2F0.
    pub fn is_xml(&self) -> bool {
        !self.is_bin
    }
    /// `MetaSave.get_isSaving` 0x32D2E0.
    pub fn is_saving(&self) -> bool {
        self.save_request.is_some()
    }
    /// `MetaSave.get_isLoading` 0x1E73D0.
    pub fn is_loading(&self) -> bool {
        self.load_request.is_some()
    }
    /// `MetaSave.get_isBroken` 0x32D2C0.
    pub fn is_broken(&self) -> bool {
        self.state & ms::BROKEN_MASK != 0
    }
    /// `MetaSave.get_shouldBeVisible` 0x32D320.
    pub fn should_be_visible(&self) -> bool {
        self.state & ms::INVISIBLE_MASK == 0
    }
    /// `MetaSave.get_shouldBeInPool` 0x32D300.
    pub fn should_be_in_pool(&self) -> bool {
        self.state & ms::SHOULD_BE_IN_POOL_MASK != 0
    }
    /// `MetaSave.ResetPooledObject` 0x32CF50: back to a blank (logged when it was not a pooled one).
    pub fn reset_pooled_object(&mut self, log_error: &mut dyn FnMut(&str)) {
        if self.state & ms::SHOULD_BE_IN_POOL_MASK == 0 {
            log_error(&format!("MetaSave is not in the pool: {}", self.state));
        }
        self.file_name = String::new();
        self.last_file_write_time = 0;
        self.last_disk_sync_time = -1.0;
        self.last_texture_load_time = -1.0;
        self.last_save_data_time = -1.0;
        self.exists_on_disk = false;
        self.path = String::new();
    }
    /// `MetaSave.IComparable<MetaSave>.CompareTo` 0x32D0B0: whole ones first, then the newest file, then the
    /// instance id.
    pub fn compare_to(&self, other: &MetaSave) -> i32 {
        let a = ((self.state & 0x1E0) != 0) as i32;
        let b = ((other.state & 0x1E0) != 0) as i32;
        let c = a.cmp(&b) as i32;
        if c != 0 {
            return c;
        }
        let c = -(self.last_file_write_time.cmp(&other.last_file_write_time) as i32);
        if c != 0 {
            return c;
        }
        self.instance_id.cmp(&other.instance_id) as i32
    }
}

/// `FuryStudios.FurySDK.AsyncRequestState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestState {
    Idle = 0,
    Running = 1,
    Success = 2,
    Fail = 4,
}

/// `FuryStudios.FurySDK.IStorageContainer` (`Platform.SDK.GetStorage("storage")`, middleware): files under the
/// save folder by relative path, each call an asynchronous request.
pub trait StorageContainer {
    fn load_bytes(&mut self, file_name: &str) -> RequestId;
    fn save_bytes(&mut self, file_name: &str, data: Vec<u8>) -> RequestId;
    fn delete(&mut self, file_name: &str) -> RequestId;
    fn file_exists(&mut self, file_name: &str) -> RequestId;
    fn list_files(&mut self) -> RequestId;
    /// `IEnumerator.MoveNext` on the request: still running.
    fn move_next(&mut self, r: RequestId) -> bool;
    fn state(&self, r: RequestId) -> RequestState;
    /// `IAsyncRequest<byte[]>.Result`.
    fn bytes_result(&self, r: RequestId) -> Option<Vec<u8>>;
    /// `IAsyncRequest<bool>.Result`.
    fn bool_result(&self, r: RequestId) -> bool;
    /// `IAsyncRequest<IList<string>>.Result`.
    fn list_result(&self, r: RequestId) -> Option<Vec<String>>;
}

/// What the save system reaches outside itself.
pub trait SaveSystemHost {
    fn storage(&mut self) -> &mut dyn StorageContainer;
    /// `Time.unscaledTime`.
    fn unscaled_time(&self) -> f32;
    /// `DateTime.Now.GetHashCode()`.
    fn now_hash(&self) -> i32;
    /// `DateTime.UtcNow` ticks.
    fn utc_now_ticks(&self) -> i64;
    /// `File.GetLastWriteTimeUtc(Path.Combine(uiMaster.DefaultPath(""), path))` ticks.
    fn last_write_time_utc_ticks(&self, path: &str) -> i64;
    fn log(&mut self, message: &str);
    fn log_error(&mut self, message: &str);
    /// `worldMaster.anyChange`.
    fn world_any_change(&self) -> bool;
    fn set_world_any_change(&mut self, v: bool);
    /// `worldMaster.PopulateSaveData(saveData, null)`.
    fn world_populate_save_data(&mut self, save: &mut SaveData);
    /// `worldMaster.Load(saveData)`.
    fn world_load(&mut self, save: &SaveData);
    /// `worldMaster.New(saveData)`.
    fn world_new(&mut self, save: &mut SaveData);
    /// `worldMaster.SetDirty()`.
    fn world_set_dirty(&mut self);
    /// `worldMaster.defaultSaveData.defaultSaveData`.
    fn default_save_data(&self) -> SaveData;
    /// `Object.Instantiate(template)`'s instance id; `new Texture2D(512, 512)`.
    fn new_meta_save_object(&mut self) -> (i32, u64);
    /// `ImageConversion.LoadImage(texture, jpg)`.
    fn load_image(&mut self, texture: u64, jpg: Option<&[u8]>);
    /// `gameObject.SetActive`.
    fn set_active(&mut self, meta: MetaSaveId, active: bool);
    /// `onNewMetaSave(meta)` (the menu makes a card).
    fn on_new_meta_save(&mut self, meta: MetaSaveId);
    /// `meta.onStateChange()`.
    fn on_state_change(&mut self, meta: MetaSaveId);
    /// `meta.onIndexChange((byte)index)`.
    fn on_index_change(&mut self, meta: MetaSaveId, index: u8);
    /// `onSaveRequestCountChange(old, new)`.
    fn on_save_request_count_change(&mut self, old: i32, new: i32);
    /// `uiMaster.saveMenu.onTotalSaveGameCountUpdate((byte)count)`.
    fn on_total_save_game_count_update(&mut self, count: u8);
    /// `enabled = true` on the component.
    fn set_enabled(&mut self, enabled: bool);
    /// `uiMaster.DefaultPath(folder)` (G7a).
    fn default_path(&self, folder: &str) -> String;
    /// `DebugUI.instance.text += s`.
    fn debug_ui_append(&mut self, s: &str);
    fn create_directory(&mut self, path: &str);
    fn enumerate_files(&self, dir: &str) -> Vec<String>;
    fn file_exists_sync(&self, path: &str) -> bool;
    fn file_copy(&mut self, from: &str, to: &str);
    fn file_delete(&mut self, path: &str);
}

/// `SaveSystem.BootState`.
pub mod boot_state {
    pub const MOVE_ALL_CURRENT_TO_POOL: i32 = 0;
    pub const BEGIN_LOAD_SETTINGS: i32 = 1;
    pub const LOAD_SETTINGS_BYTES: i32 = 2;
    pub const CREATE_NEW_SETTINGS: i32 = 3;
    pub const TRY_LOAD_LAST_SAVE: i32 = 4;
    pub const ITERATE_LAST_META_SAVE: i32 = 5;
    pub const LOAD_SAVE_LIST: i32 = 6;
    pub const ITERATE_ALL_SAVES: i32 = 7;
    pub const VALID_LAST_SAVE: i32 = 8;
    pub const NO_VALID_LAST_SAVE: i32 = 9;
    pub const NEW_FROM_DEFAULT_SAVE: i32 = 10;
}

/// `SaveSystem.State` (the refresh).
pub mod refresh_state {
    pub const DO_NOTHING: i32 = 0;
    pub const BEGIN_REFRESH_SAVES: i32 = 1;
    pub const LIST_FILES: i32 = 2;
    pub const PROCESS_STRINGS: i32 = 3;
    pub const CHECK_EXISTING_META_SAVES: i32 = 4;
    pub const CREATE_NEW_META_FILES: i32 = 5;
    pub const PROCESS_ALL_META_FILES: i32 = 6;
    pub const DONE: i32 = 7;
}

/// What a finished storage request does (the `OnComplete` delegates).
#[derive(Clone, Debug, PartialEq)]
enum OnComplete {
    /// `<>c__DisplayClass54_0.<LoadMetaSaveFromDisk>b__0` 0x5056D0.
    Load(MetaSaveId),
    /// `<>c__DisplayClass55_1.<SaveMetaSaveToDisk>b__2` 0x505EA0.
    Save { meta: MetaSaveId, is_new_save: bool, set_as_last_save: bool },
    /// `<>c__DisplayClass65_0.<DeleteMetaSaveFromDisk>b__0` 0x506150.
    Delete,
    /// `SaveSystem.<SaveSettings>b__56_0` 0x96E150.
    SaveSettings,
}

/// `<SaveMetaSaveToDisk>g__NoPathRoutine|1` (a coroutine): a fresh unique name, tried until no file has it.
#[derive(Clone, Debug, PartialEq)]
struct NoPathRoutine {
    meta: MetaSaveId,
    set_as_last_save: bool,
    random: Random,
    file_name: String,
    path: String,
    request: Option<RequestId>,
}

/// The component.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveSystem {
    pub any_change_when_refreshing: bool,
    pub boot_state: i32,
    pub state: i32,
    /// Every meta save object (by id; where each sits is `pool` / `container`).
    pub metas: Vec<MetaSave>,
    /// `metaSavePool`'s children in order (child 0 the template).
    pub pool: Vec<MetaSaveId>,
    /// `metaSaveContainer`'s children in order.
    pub container: Vec<MetaSaveId>,
    pub settings_data: Option<SettingsData>,
    pub settings_change_time: f32,
    pub settings_save_time: f32,
    pub settings_start_change_time: f32,
    pub current_meta_save: Option<MetaSaveId>,
    pub file_list: Vec<String>,
    pub start_refresh_time: f32,
    pub index: i32,
    pub count: i32,
    pub valid_meta_save_count: i32,
    pub list_request: Option<RequestId>,
    pub byte_request: Option<RequestId>,
    pub bool_request: Option<RequestId>,
    pub save_settings_request: Option<RequestId>,
    pub active_requests_counter: i32,
    pub max_save_game_count: i32,
    pub total_save_game_count: i32,
    pending: Vec<(RequestId, OnComplete)>,
    routines: Vec<NoPathRoutine>,
}

impl SaveSystem {
    /// `SaveSystem..ctor` 0x96E320, with the scene's pool template (`template_instance_id`).
    pub fn new(template_instance_id: i32) -> SaveSystem {
        SaveSystem {
            any_change_when_refreshing: true,
            boot_state: 0,
            state: 0,
            metas: vec![MetaSave::template(template_instance_id)],
            pool: vec![0],
            container: vec![],
            settings_data: None,
            settings_change_time: 0.0,
            settings_save_time: 0.0,
            settings_start_change_time: 0.0,
            current_meta_save: None,
            file_list: vec![],
            start_refresh_time: 0.0,
            index: 0,
            count: 0,
            valid_meta_save_count: 0,
            list_request: None,
            byte_request: None,
            bool_request: None,
            save_settings_request: None,
            active_requests_counter: 0,
            max_save_game_count: i32::MAX,
            total_save_game_count: 0,
            pending: vec![],
            routines: vec![],
        }
    }

    /// `Transform.SetParent(parent)` for a meta: to the end of the other list (no-op when already there).
    fn set_parent(&mut self, meta: MetaSaveId, to_pool: bool) {
        let in_pool = self.pool.contains(&meta);
        if in_pool == to_pool && (in_pool || self.container.contains(&meta)) {
            return;
        }
        self.pool.retain(|&m| m != meta);
        self.container.retain(|&m| m != meta);
        if to_pool {
            self.pool.push(meta);
        } else {
            self.container.push(meta);
        }
    }

    /// `Transform.SetSiblingIndex` in the container (clamped to the ends).
    fn set_container_index(&mut self, meta: MetaSaveId, index: i32) {
        let Some(at) = self.container.iter().position(|&m| m == meta) else { return };
        self.container.remove(at);
        let i = index.clamp(0, self.container.len() as i32) as usize;
        self.container.insert(i, meta);
    }

    /// `Transform.GetSiblingIndex` in the container.
    fn container_index(&self, meta: MetaSaveId) -> i32 {
        self.container.iter().position(|&m| m == meta).map_or(0, |i| i as i32)
    }

    fn notify_requests(&mut self, host: &mut dyn SaveSystemHost, delta: i32) {
        let old = self.active_requests_counter;
        self.active_requests_counter = old + delta;
        host.on_save_request_count_change(old, self.active_requests_counter);
    }

    /// `SaveSystem.ChangeState(metaSave, state)` 0x96B420: the state set and told; a newly broken one has its
    /// data cleared; shown or hidden (the visible count kept); one that belongs in the pool moved there.
    fn change_state(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, state: u32) {
        let was_visible = self.metas[meta].should_be_visible();
        let was_broken = self.metas[meta].is_broken();
        self.metas[meta].state = state;
        host.on_state_change(meta);
        let visible = self.metas[meta].should_be_visible();
        if !was_broken && self.metas[meta].is_broken() {
            self.metas[meta].save_data.as_mut().expect("ChangeState: no save data").clear();
        }
        self.metas[meta].active = visible;
        host.set_active(meta, visible);
        if was_visible != visible {
            self.valid_meta_save_count += if visible { 1 } else { -1 };
        }
        if self.metas[meta].should_be_in_pool() {
            self.set_parent(meta, true);
        }
    }

    /// `SaveSystem.GetSaveFilePath(fileName)` 0x96BC00: `Saves\<name>`.
    pub fn get_save_file_path(file_name: &str) -> String {
        format!("{SAVE_FILE_DIRECTORY_NAME}{DIRECTORY_SEPARATOR}{file_name}")
    }

    /// `SaveSystem.GetUniqueSaveName` 0x96BCA0: `Town` + 16 letters of [`SAVE_CHARS`] from a
    /// `Unity.Mathematics.Random` seeded with `DateTime.Now.GetHashCode()`, filled from the last + `.scape`.
    pub fn get_unique_save_name(now_hash: i32) -> String {
        let mut random = Random::new(now_hash as u32);
        Self::random_name(&mut random)
    }

    fn random_name(random: &mut Random) -> String {
        let chars: Vec<char> = SAVE_CHARS.chars().collect();
        let mut name = vec![' '; SAVE_NAME_CHARS];
        for i in (0..SAVE_NAME_CHARS).rev() {
            name[i] = chars[random.next_int_range(0, chars.len() as i32) as usize];
        }
        format!("{SAVE_FILE_PREFIX}{}{XML_EXTENSION}", name.iter().collect::<String>())
    }

    /// `SaveSystem.GetNewMetaSave(state, path)` 0x96B9F0: a meta from the pool (a copy of the template when only
    /// it is left, with a new texture and a card; else the last one, reset), its path and kind set, into the
    /// container, in `state`.
    pub fn get_new_meta_save(&mut self, host: &mut dyn SaveSystemHost, state: u32, path: &str) -> MetaSaveId {
        let meta = if self.pool.len() == 1 {
            let template = self.metas[self.pool[0]].clone();
            let (instance_id, texture) = host.new_meta_save_object();
            let id = self.metas.len();
            self.metas.push(MetaSave { instance_id, texture, ..template });
            self.pool.push(id);
            host.on_new_meta_save(id);
            id
        } else {
            let id = *self.pool.last().unwrap();
            let mut log = |m: &str| host.log_error(m);
            self.metas[id].reset_pooled_object(&mut log);
            id
        };
        let m = &mut self.metas[meta];
        m.is_bin = path.ends_with(BIN_EXTENSION);
        m.path = path.to_string();
        m.file_name = path.rsplit(['\\', '/']).next().unwrap_or("").to_string();
        self.set_parent(meta, false);
        self.change_state(host, meta, state);
        meta
    }

    /// `SaveSystem.GetAndMaybeCreateCurrent` 0x96B8A0.
    pub fn get_and_maybe_create_current(&mut self, host: &mut dyn SaveSystemHost) -> MetaSaveId {
        if self.current_meta_save.is_none() {
            self.current_meta_save = Some(self.get_new_meta_save(host, ms::NOT_YET_SAVED, NO_PATH));
        }
        self.current_meta_save.unwrap()
    }

    /// `SaveSystem.GetMetaSaveCount` 0x96B960.
    pub fn get_meta_save_count(&self) -> i32 {
        self.container.len() as i32
    }

    /// `SaveSystem.GetMetaSave(index)` 0x96B990.
    pub fn get_meta_save(&self, index: i32) -> MetaSaveId {
        self.container[index as usize]
    }

    /// `SaveSystem.SetSiblingIndex(metaSave, index)` 0x96DDB0: a binary search's not-found result taken as its
    /// insertion point; the card told.
    fn set_sibling_index(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, index: i32) {
        let i = if index < 0 { !index } else { index };
        self.set_container_index(meta, i);
        host.on_index_change(meta, i as u8);
    }

    /// `Os.Utils.ExtraMath.BinarySearchComponent<MetaSave>(transform, item, marginMin, marginMax)` (a generic:
    /// its shared body at 0x30D540, not an index row):
    /// over the container's children `marginMin ..= count - marginMax - 1` by `CompareTo`; not found: the
    /// complement of where it would go.
    fn binary_search(&self, item: MetaSaveId, margin_min: i32, margin_max: i32) -> i32 {
        let (mut lo, mut hi) = (margin_min, self.container.len() as i32 - margin_max - 1);
        let (mut mid, mut d) = (0i32, 0i32);
        while lo <= hi {
            mid = (lo + hi) / 2;
            let c = self.metas[self.container[mid as usize]].compare_to(&self.metas[item]);
            d = -c;
            if d == 0 {
                return mid;
            }
            if d < 0 {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        !(if d <= 0 { mid } else { mid + 1 })
    }

    /// `SaveSystem.LoadMetaSaveFromDisk(metaSave)` 0x96CB80: unless it is loading or saving (logged), its file
    /// read; on completion [`Self::complete`] fills it.
    fn load_meta_save_from_disk(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) {
        let m = &self.metas[meta];
        if m.is_loading() {
            host.log_error(&format!("MetaSave already loading {}", m.path));
            return;
        }
        if m.is_saving() {
            host.log_error(&format!("Trying to load MetaSave that is saving {}", m.path));
            return;
        }
        let path = m.path.clone();
        let r = host.storage().load_bytes(&path);
        let t = host.unscaled_time();
        let m = &mut self.metas[meta];
        m.last_disk_sync_time = t;
        m.load_request = Some(r);
        self.pending.push((r, OnComplete::Load(meta)));
    }

    /// The bytes of a save as its kind writes them.
    fn serialize(meta: &MetaSave) -> Vec<u8> {
        let data = meta.save_data.as_ref().expect("SaveMetaSaveToDisk: no save data");
        if meta.is_bin {
            // STANDIN: .binscape (BinaryFormatter) writing is row G3c; the game never writes one (shouldBeBin false)
            unimplemented!("BinaryFormatter of SaveData (G3c)")
        }
        save_xml::write_save(data)
    }

    /// `<>c__DisplayClass55_0.<SaveMetaSaveToDisk>g__SaveSave|0(isNewSave)` 0x505B50: the save serialized, its
    /// sync and write times stamped, written; on completion [`Self::complete`].
    fn save_save(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, is_new_save: bool, set_as_last_save: bool) {
        let bytes = Self::serialize(&self.metas[meta]);
        let t = host.unscaled_time();
        let ticks = host.utc_now_ticks();
        let m = &mut self.metas[meta];
        m.last_disk_sync_time = t;
        m.last_file_write_time = ticks;
        let path = m.path.clone();
        let r = host.storage().save_bytes(&path, bytes);
        self.notify_requests(host, 1);
        self.pending.push((r, OnComplete::Save { meta, is_new_save, set_as_last_save }));
    }

    /// `SaveSystem.SaveMetaSaveToDisk(metaSave, setAsLastSave)` 0x96D930: unless it is loading or saving
    /// (logged): one on disk is written; one without a file first gets a unique name (a coroutine).
    pub fn save_meta_save_to_disk(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, set_as_last_save: bool) {
        let m = &self.metas[meta];
        if m.is_loading() {
            host.log_error(&format!("Trying to save MetaSave that is loading {}", m.path));
            return;
        }
        if m.is_saving() {
            host.log_error(&format!("MetaSave already saving {}", m.path));
            return;
        }
        if m.exists_on_disk {
            self.save_save(host, meta, false, set_as_last_save);
            return;
        }
        // `StartCoroutine(NoPathRoutine())`: its first step runs now
        let random = Random::new(host.now_hash() as u32);
        self.routines.push(NoPathRoutine { meta, set_as_last_save, random, file_name: String::new(), path: String::new(), request: None });
        let i = self.routines.len() - 1;
        if self.step_no_path(host, i) {
            self.routines.remove(i);
        }
    }

    /// One step of `NoPathRoutine` (`MoveNext` 0x501A70); true when it is done.
    fn step_no_path(&mut self, host: &mut dyn SaveSystemHost, i: usize) -> bool {
        loop {
            let r = &mut self.routines[i];
            match r.request {
                None => {
                    let meta = r.meta;
                    self.metas[meta].is_bin = false;
                    let r = &mut self.routines[i];
                    r.file_name = Self::random_name(&mut r.random);
                    r.path = Self::get_save_file_path(&r.file_name);
                    let path = r.path.clone();
                    let req = host.storage().file_exists(&path);
                    self.routines[i].request = Some(req);
                }
                Some(req) => {
                    if host.storage().move_next(req) {
                        return false;
                    }
                    if !host.storage().bool_result(req) {
                        let r = self.routines[i].clone();
                        self.metas[r.meta].path = r.path;
                        self.metas[r.meta].file_name = r.file_name;
                        self.save_save(host, r.meta, true, r.set_as_last_save);
                        return true;
                    }
                    self.routines[i].request = None;
                }
            }
        }
    }

    /// The running coroutines' next steps (the host calls this once a frame).
    pub fn update_coroutines(&mut self, host: &mut dyn SaveSystemHost) {
        let mut i = 0;
        while i < self.routines.len() {
            if self.step_no_path(host, i) {
                self.routines.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// A storage request finished: its `OnComplete`.
    pub fn complete(&mut self, host: &mut dyn SaveSystemHost, request: RequestId) {
        let Some(at) = self.pending.iter().position(|p| p.0 == request) else { return };
        let (_, what) = self.pending.remove(at);
        match what {
            OnComplete::Load(meta) => self.on_loaded(host, meta, request),
            OnComplete::Save { meta, is_new_save, set_as_last_save } => self.on_saved(host, meta, request, is_new_save, set_as_last_save),
            OnComplete::Delete => {
                if host.storage().state(request) == RequestState::Success {
                    self.total_save_game_count -= 1;
                    host.on_total_save_game_count_update(self.total_save_game_count as u8);
                }
                self.notify_requests(host, -1);
            }
            OnComplete::SaveSettings => {
                host.log("Saved Settings");
                self.save_settings_request = None;
                self.notify_requests(host, -1);
            }
        }
    }

    /// `<LoadMetaSaveFromDisk>b__0` 0x5056D0: the file deserialized; unreadable: fresh data, failed; else the
    /// file's write time, the texture, up to date; shown as its state says, on disk, not loading.
    fn on_loaded(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, request: RequestId) {
        let bytes = host.storage().bytes_result(request).unwrap_or_default();
        let data = if self.metas[meta].is_bin {
            // STANDIN: .binscape (BinaryFormatter) reading is row G3c
            None
        } else {
            match save_xml::read_save(&bytes) {
                Ok(d) => Some(d),
                Err(e) => {
                    // the serializer throws out of the callback: the meta is left loading
                    host.log_error(&format!("InvalidOperationException: {}: {}", self.metas[meta].path, e.0));
                    return;
                }
            }
        };
        self.metas[meta].save_data = data;
        if self.metas[meta].save_data.is_none() {
            self.metas[meta].save_data = Some(SaveData::default());
            self.change_state(host, meta, ms::FAILED_TO_LOAD);
            return;
        }
        let path = self.metas[meta].path.clone();
        self.metas[meta].last_file_write_time = host.last_write_time_utc_ticks(&path);
        self.metas[meta].last_save_data_time = host.unscaled_time();
        self.maybe_update_texture(host, meta);
        self.change_state(host, meta, ms::UP_TO_DATE);
        let visible = self.metas[meta].should_be_visible();
        self.metas[meta].active = visible;
        host.set_active(meta, visible);
        self.metas[meta].exists_on_disk = true;
        self.metas[meta].load_request = None;
    }

    /// `<SaveMetaSaveToDisk>b__2` 0x505EA0: logged; a new file that wrote counts; on disk, not saving, up to
    /// date; the current town's name becomes the settings' last save.
    fn on_saved(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId, request: RequestId, is_new_save: bool, set_as_last_save: bool) {
        host.log(&format!("Saved: {}", self.metas[meta].path));
        if is_new_save & (host.storage().state(request) == RequestState::Success) {
            self.total_save_game_count += 1;
            host.on_total_save_game_count_update(self.total_save_game_count as u8);
        }
        self.metas[meta].exists_on_disk = true;
        self.metas[meta].save_request = None;
        self.notify_requests(host, -1);
        self.change_state(host, meta, ms::UP_TO_DATE);
        if !set_as_last_save || Some(meta) != self.current_meta_save {
            return;
        }
        let name = self.metas[meta].file_name.clone();
        let settings = self.settings_data.as_mut().expect("no settings");
        if settings.last_save.as_deref() == Some(name.as_str()) {
            return;
        }
        settings.last_save = Some(name);
        self.set_settings_dirty(host);
    }

    /// `SaveSystem.DeleteMetaSaveFromDisk(metaSave, out wasCurrent)` 0x96B520: only a file on disk (logged):
    /// the current town is replaced by a new one; the file deleted, the meta deleted and cleared.
    pub fn delete_meta_save_from_disk(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) -> (bool, bool) {
        if !self.metas[meta].exists_on_disk {
            host.log_error(&format!("Trying to delete save that does not exist on disk {}", self.metas[meta].path));
            return (false, false);
        }
        let was_current = Some(meta) == self.current_meta_save;
        if was_current {
            self.current_meta_save = None;
            let mut data = self.metas[meta].save_data.take().unwrap_or_default();
            host.world_new(&mut data);
            host.world_load(&data);
            self.metas[meta].save_data = Some(data);
        }
        let path = self.metas[meta].path.clone();
        let r = host.storage().delete(&path);
        self.notify_requests(host, 1);
        self.pending.push((r, OnComplete::Delete));
        self.change_state(host, meta, ms::DELETED);
        if let Some(d) = self.metas[meta].save_data.as_mut() {
            d.clear();
        }
        (true, was_current)
    }

    /// `SaveSystem.DuplicateSave(oldMetaSave)` 0x96B760: a new meta with a copy of the old one's data (and
    /// texture), placed after it, written under a new name.
    pub fn duplicate_save(&mut self, host: &mut dyn SaveSystemHost, old: MetaSaveId) {
        self.count += 1;
        let meta = self.get_new_meta_save(host, ms::NEW_FROM_DUPLICATE, NO_PATH);
        let src = self.metas[old].save_data.clone().unwrap_or_default();
        self.metas[meta].save_data.as_mut().expect("no save data").copy_from(&src);
        let t = host.unscaled_time();
        self.metas[meta].last_save_data_time = t;
        if self.metas[meta].last_texture_load_time <= t {
            let m = &self.metas[meta];
            host.load_image(m.texture, m.save_data.as_ref().and_then(|d| d.big_jpg.as_deref()));
            self.metas[meta].last_texture_load_time = host.unscaled_time();
        }
        let i = self.container_index(old) + 1;
        let i = if i < 0 { !i } else { i };
        self.set_container_index(meta, i);
        host.on_index_change(meta, i as u8);
        self.save_meta_save_to_disk(host, meta, false);
    }

    /// `SaveSystem.IsMetaSaveLoading(metaSave)` 0x96BE00: loading, or (during a refresh, for one not synced since
    /// it began) made to load now.
    pub fn is_meta_save_loading(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) -> bool {
        if !self.metas[meta].is_loading() {
            if 6 < self.state || self.start_refresh_time <= self.metas[meta].last_disk_sync_time {
                return false;
            }
            self.load_meta_save_from_disk(host, meta);
        }
        true
    }

    /// `SaveSystem.IterateBoot(keepGoing)` 0x96BE60: one step of the boot; false when it is over.
    pub fn iterate_boot(&mut self, host: &mut dyn SaveSystemHost) -> bool {
        use boot_state as b;
        match self.boot_state {
            b::MOVE_ALL_CURRENT_TO_POOL => {
                if self.container.is_empty() {
                    self.boot_state += 1;
                } else {
                    let meta = *self.container.last().unwrap();
                    self.change_state(host, meta, ms::RETURNED_TO_POOL);
                }
            }
            b::BEGIN_LOAD_SETTINGS => match self.bool_request {
                Some(r) => {
                    if !host.storage().move_next(r) {
                        self.boot_state = (!host.storage().bool_result(r)) as i32 + 2;
                        self.bool_request = None;
                    }
                }
                None => self.bool_request = Some(host.storage().file_exists(SETTINGS_FILE_NAME)),
            },
            b::LOAD_SETTINGS_BYTES => match self.byte_request {
                None => self.byte_request = Some(host.storage().load_bytes(SETTINGS_FILE_NAME)),
                Some(r) => {
                    if !host.storage().move_next(r) {
                        let bytes = host.storage().bytes_result(r).expect("Sett.ings: no bytes");
                        // (an unreadable file throws here, as in the game)
                        let settings = save_xml::read_settings(&bytes).unwrap_or_else(|e| panic!("Sett.ings: {e:?}"));
                        self.settings_data = Some(settings);
                        let s = self.settings_data.as_mut().unwrap();
                        s.last_save = s.last_save.as_ref().map(|l| l.replace('/', "\\"));
                        self.set_settings_dirty(host);
                        self.maybe_update_old_settings(host);
                        self.boot_state = b::TRY_LOAD_LAST_SAVE;
                        self.byte_request = None;
                    }
                }
            },
            b::CREATE_NEW_SETTINGS => {
                self.settings_data = Some(SettingsData { version: 2, ..SettingsData::default() });
                self.boot_state = b::LOAD_SAVE_LIST;
            }
            b::TRY_LOAD_LAST_SAVE => match self.bool_request {
                Some(r) => {
                    if !host.storage().move_next(r) {
                        if !host.storage().bool_result(r) {
                            self.boot_state = b::LOAD_SAVE_LIST;
                        } else {
                            let last = self.settings_data.as_ref().unwrap().last_save.clone().unwrap_or_default();
                            let meta = self.get_new_meta_save(host, ms::NEEDS_UPDATING, &Self::get_save_file_path(&last));
                            self.current_meta_save = Some(meta);
                            self.load_meta_save_from_disk(host, meta);
                            self.boot_state = b::ITERATE_LAST_META_SAVE;
                        }
                        self.bool_request = None;
                    }
                }
                None => {
                    let last = self.settings_data.as_ref().expect("no settings").last_save.clone().unwrap_or_default();
                    self.bool_request = Some(host.storage().file_exists(&Self::get_save_file_path(&last)));
                }
            },
            b::ITERATE_LAST_META_SAVE => {
                let cur = self.current_meta_save.expect("no current meta save");
                if !self.metas[cur].is_loading() {
                    if self.metas[cur].is_broken() {
                        self.current_meta_save = None;
                        self.boot_state = b::LOAD_SAVE_LIST;
                    } else {
                        self.boot_state = b::VALID_LAST_SAVE;
                    }
                }
            }
            b::LOAD_SAVE_LIST => {
                if self.state == refresh_state::DO_NOTHING {
                    self.state = refresh_state::BEGIN_REFRESH_SAVES;
                } else if self.state < refresh_state::CHECK_EXISTING_META_SAVES {
                    self.iterate_refresh(host);
                } else if !self.file_list.is_empty() {
                    self.boot_state = b::ITERATE_ALL_SAVES;
                } else {
                    self.boot_state = b::NEW_FROM_DEFAULT_SAVE;
                    let meta = self.get_new_meta_save(host, ms::NOT_YET_SAVED, NO_PATH);
                    let def = host.default_save_data();
                    self.metas[meta].save_data.as_mut().expect("no save data").copy_from(&def);
                    self.load_save(host, Some(meta));
                    host.world_set_dirty();
                    self.change_state(host, meta, ms::NOT_YET_SAVED);
                }
            }
            b::ITERATE_ALL_SAVES => {
                if !self.iterate_refresh(host) {
                    self.boot_state = b::NO_VALID_LAST_SAVE;
                }
            }
            b::VALID_LAST_SAVE | b::NO_VALID_LAST_SAVE => {
                if self.state == refresh_state::DO_NOTHING {
                    self.state = refresh_state::BEGIN_REFRESH_SAVES;
                }
                return false;
            }
            b::NEW_FROM_DEFAULT_SAVE => {
                self.state = refresh_state::DONE;
                return false;
            }
            _ => {}
        }
        true
    }

    /// `Path.GetDirectoryName` of a relative storage path.
    fn directory_name(path: &str) -> &str {
        match path.rfind(['\\', '/']) {
            Some(i) => &path[..i],
            None => "",
        }
    }

    /// `SaveSystem.IterateRefresh(keepGoing)` 0x96C5A0: one step of the refresh; false when it is done.
    pub fn iterate_refresh(&mut self, host: &mut dyn SaveSystemHost) -> bool {
        use refresh_state as s;
        match self.state {
            s::BEGIN_REFRESH_SAVES => {
                self.any_change_when_refreshing = false;
                self.start_refresh_time = host.unscaled_time();
                self.state += 1;
            }
            s::LIST_FILES => match self.list_request {
                None => self.list_request = Some(host.storage().list_files()),
                Some(r) => {
                    if host.storage().move_next(r) {
                        return true;
                    }
                    self.file_list.clear();
                    if let Some(l) = host.storage().list_result(r) {
                        self.file_list.extend(l);
                    }
                    self.list_request = None;
                    self.index = 0;
                    self.count = self.file_list.len() as i32;
                    self.total_save_game_count = 0;
                    self.state += 1;
                }
            },
            s::PROCESS_STRINGS => {
                if self.index < self.count {
                    let f = &self.file_list[self.index as usize];
                    if (f.ends_with(XML_EXTENSION) || f.ends_with(BIN_EXTENSION)) && Self::directory_name(f) == SAVE_FILE_DIRECTORY_NAME {
                        self.total_save_game_count += 1;
                        self.index += 1;
                    } else {
                        self.file_list.remove(self.index as usize);
                        self.count -= 1;
                    }
                } else {
                    self.index = 0;
                    self.count = self.container.len() as i32;
                    self.state += 1;
                }
            }
            s::CHECK_EXISTING_META_SAVES => {
                if self.index < self.count {
                    let meta = self.container[self.index as usize];
                    if self.metas[meta].state != ms::NOT_YET_SAVED {
                        let path = self.metas[meta].path.clone();
                        match self.file_list.iter().position(|f| *f == path) {
                            Some(i) => {
                                self.file_list.remove(i);
                            }
                            None => {
                                self.metas[meta].last_file_write_time = 0;
                                self.change_state(host, meta, ms::DID_NOT_EXIST_ON_DISK);
                                self.any_change_when_refreshing = true;
                                self.count -= 1;
                                return true;
                            }
                        }
                    }
                    if Some(meta) != self.current_meta_save {
                        self.load_meta_save_from_disk(host, meta);
                    }
                    self.index += 1;
                } else {
                    self.index = 0;
                    self.count = self.file_list.len() as i32;
                    self.state = s::CREATE_NEW_META_FILES;
                }
            }
            s::CREATE_NEW_META_FILES => {
                if self.index < self.count {
                    let path = self.file_list[self.index as usize].clone();
                    let meta = self.get_new_meta_save(host, ms::NEEDS_UPDATING, &path);
                    self.load_meta_save_from_disk(host, meta);
                    self.any_change_when_refreshing = true;
                    self.index += 1;
                } else {
                    self.index = 0;
                    self.count = self.container.len() as i32;
                    if let Some(cur) = self.current_meta_save {
                        self.set_sibling_index(host, cur, 0);
                    }
                    self.state += 1;
                }
            }
            s::PROCESS_ALL_META_FILES => {
                if self.count <= self.index {
                    self.state = s::DONE;
                    return true;
                }
                let meta = self.get_meta_save(self.index);
                if self.metas[meta].is_loading() {
                    return true;
                }
                let first = self.current_meta_save.is_some() as i32;
                if first < self.index {
                    let i = self.binary_search(meta, first, self.count - self.index);
                    self.set_sibling_index(host, meta, i);
                }
                self.index += 1;
            }
            s::DONE => return false,
            _ => {}
        }
        true
    }

    /// `SaveSystem.LoadSave(metaSave)` 0x96CD60: another town: the current one saved if changed (else, never
    /// written, dropped), this one current, remembered as the last save, and loaded into the world.
    pub fn load_save(&mut self, host: &mut dyn SaveSystemHost, meta: Option<MetaSaveId>) -> bool {
        if meta == self.current_meta_save {
            return false;
        }
        if !self.maybe_save_current(host) {
            if let Some(cur) = self.current_meta_save {
                if !self.metas[cur].exists_on_disk {
                    self.metas[cur].save_data.as_mut().expect("no save data").clear();
                    self.change_state(host, cur, ms::DID_NOT_EXIST_ON_DISK);
                }
            }
        }
        self.current_meta_save = None;
        self.current_meta_save = meta;
        let meta = meta.expect("LoadSave: null");
        let name = self.metas[meta].file_name.clone();
        let settings = self.settings_data.as_mut().expect("no settings");
        if settings.last_save.as_deref() != Some(name.as_str()) {
            settings.last_save = Some(name);
            self.set_settings_dirty(host);
        }
        host.log(&format!("Loading: {}", self.metas[meta].path));
        let data = self.metas[meta].save_data.clone().unwrap_or_default();
        host.world_load(&data);
        true
    }

    /// `SaveSystem.MaybeSaveAndUnloadCurrent` 0x96CFC0.
    pub fn maybe_save_and_unload_current(&mut self, host: &mut dyn SaveSystemHost) {
        if !self.maybe_save_current(host) {
            if let Some(cur) = self.current_meta_save {
                if !self.metas[cur].exists_on_disk {
                    self.metas[cur].save_data.as_mut().expect("no save data").clear();
                    self.change_state(host, cur, ms::DID_NOT_EXIST_ON_DISK);
                }
            }
        }
        self.current_meta_save = None;
    }

    /// The current town's save data refreshed from the world and its texture updated (shared by the save paths).
    fn populate_current(&mut self, host: &mut dyn SaveSystemHost) -> MetaSaveId {
        let meta = self.get_and_maybe_create_current(host);
        let mut data = self.metas[meta].save_data.take().unwrap_or_default();
        host.world_populate_save_data(&mut data);
        self.metas[meta].save_data = Some(data);
        self.metas[meta].last_save_data_time = host.unscaled_time();
        meta
    }

    fn load_texture_if_stale(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) {
        let m = &self.metas[meta];
        if m.last_texture_load_time <= m.last_save_data_time {
            host.load_image(m.texture, m.save_data.as_ref().and_then(|d| d.big_jpg.as_deref()));
            self.metas[meta].last_texture_load_time = host.unscaled_time();
        }
    }

    /// `SaveSystem.MaybeSaveCurrent` 0x96D060: a changed world written to the current town (made when there is
    /// none) as the last save.
    pub fn maybe_save_current(&mut self, host: &mut dyn SaveSystemHost) -> bool {
        if !host.world_any_change() {
            return false;
        }
        let meta = self.populate_current(host);
        self.save_meta_save_to_disk(host, meta, true);
        let cur = self.current_meta_save.expect("no current");
        self.load_texture_if_stale(host, cur);
        host.set_world_any_change(false);
        true
    }

    /// `SaveSystem.MaybeSaveSettings(margin)` 0x96D120: settings changed since the last write, and for longer than
    /// `margin`, are written.
    pub fn maybe_save_settings(&mut self, host: &mut dyn SaveSystemHost, margin: f32) -> bool {
        let start = self.settings_start_change_time;
        if self.settings_save_time <= start && start != self.settings_save_time && start + margin < host.unscaled_time() {
            self.save_settings(host);
            return true;
        }
        false
    }

    /// `SaveSystem.MaybeUpdateOldSettings` 0x96D170: settings from before version 2 have their towns moved from
    /// the old folder into `Saves` (unless a file of that name is there); version 2 either way.
    fn maybe_update_old_settings(&mut self, host: &mut dyn SaveSystemHost) {
        let version = self.settings_data.as_ref().expect("no settings").version;
        if version != 0 {
            self.settings_data.as_mut().unwrap().version = 2;
            return;
        }
        host.debug_ui_append("Updating settings and moving save files\n");
        let old = host.default_path("");
        let new = host.default_path(SAVE_FILE_DIRECTORY_NAME);
        host.log(&format!("Moving files from {old} to {new}"));
        host.create_directory(&new);
        for f in host.enumerate_files(&old) {
            let ext = f.rfind('.').map(|i| &f[i..]).filter(|e| !e.contains(['\\', '/'])).unwrap_or("");
            if ext != XML_EXTENSION {
                continue;
            }
            let name = f.rsplit(['\\', '/']).next().unwrap_or("");
            let to = format!("{new}{DIRECTORY_SEPARATOR}{name}");
            if !host.file_exists_sync(&to) {
                host.file_copy(&f, &to);
                host.file_delete(&f);
            }
        }
        self.set_settings_dirty(host);
        self.settings_data.as_mut().unwrap().version = 2;
    }

    /// `SaveSystem.MaybeUpdateTexture(metaSave)` 0x96D4E0.
    pub fn maybe_update_texture(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) -> bool {
        let m = &self.metas[meta];
        if m.last_save_data_time < m.last_texture_load_time {
            return false;
        }
        host.load_image(m.texture, m.save_data.as_ref().expect("no save data").big_jpg.as_deref());
        self.metas[meta].last_texture_load_time = host.unscaled_time();
        true
    }

    /// `SaveSystem.OnApplicationQuit` 0x96D530.
    pub fn on_application_quit(&mut self, host: &mut dyn SaveSystemHost) {
        self.maybe_save_current(host);
        self.maybe_save_settings(host, 0.0);
    }

    /// `SaveSystem.OnSaveMenuOpen` 0x96D590: a changed (or current) town written now and put on top, the
    /// refresh restarted, the town after it sorted in.
    pub fn on_save_menu_open(&mut self, host: &mut dyn SaveSystemHost) {
        if self.current_meta_save.is_none() && !host.world_any_change() {
            return;
        }
        self.populate_current(host);
        let cur = self.current_meta_save.expect("no current");
        self.load_texture_if_stale(host, cur);
        self.state = refresh_state::BEGIN_REFRESH_SAVES;
        host.set_enabled(true);
        self.save_meta_save_to_disk(host, cur, false);
        if let Some(cur) = self.current_meta_save {
            self.set_container_index(cur, 0);
            host.on_index_change(cur, 0);
        }
        let n = self.container.len() as i32;
        let b = self.current_meta_save.is_some() as i32;
        if n <= b + 1 {
            return;
        }
        let meta = self.get_meta_save(b);
        let mut i = self.binary_search(meta, b, 0);
        if i < 0 {
            i = !i;
        }
        self.set_container_index(meta, i);
        host.on_index_change(meta, i as u8);
    }

    /// `SaveSystem.PopulateSaveData(metaSave)` 0x96D850.
    pub fn populate_save_data(&mut self, host: &mut dyn SaveSystemHost, meta: MetaSaveId) {
        let mut data = self.metas[meta].save_data.take().unwrap_or_default();
        host.world_populate_save_data(&mut data);
        self.metas[meta].save_data = Some(data);
        self.metas[meta].last_save_data_time = host.unscaled_time();
    }

    /// `SaveSystem.ResetBoot` 0x96D8A0.
    pub fn reset_boot(&mut self) {
        self.boot_state = 0;
        self.index = 0;
        self.count = 0;
        self.current_meta_save = None;
        self.state = 0;
    }

    /// `SaveSystem.SaveSettings` 0x96DA70: the settings serialized and written; on completion
    /// [`Self::complete`].
    pub fn save_settings(&mut self, host: &mut dyn SaveSystemHost) {
        let bytes = save_xml::write_settings(self.settings_data.as_ref().expect("no settings"));
        let r = host.storage().save_bytes(SETTINGS_FILE_NAME, bytes);
        self.save_settings_request = Some(r);
        self.notify_requests(host, 1);
        self.pending.push((r, OnComplete::SaveSettings));
        self.settings_save_time = host.unscaled_time();
    }

    /// `SaveSystem.SetCurrentMetaSave(metaSave)` 0x96DCD0.
    pub fn set_current_meta_save(&mut self, host: &mut dyn SaveSystemHost, meta: Option<MetaSaveId>) {
        if self.current_meta_save.is_some() {
            host.log_error("currentMetaSave is not null");
        }
        self.current_meta_save = meta;
    }

    /// `SaveSystem.SetSettingsDirty` 0x96DD80.
    pub fn set_settings_dirty(&mut self, host: &mut dyn SaveSystemHost) {
        if self.settings_change_time <= self.settings_save_time {
            self.settings_start_change_time = host.unscaled_time();
        }
        self.settings_change_time = host.unscaled_time();
    }

    /// `SaveSystem.SortAll` 0x96DE30: each town from the second inserted by `CompareTo` among those before it
    /// (the search stopping one short, as in the game), then the current one on top.
    pub fn sort_all(&mut self, host: &mut dyn SaveSystemHost) {
        let n = self.container.len() as i32;
        for i in 1..n {
            let meta = self.container[i as usize];
            let idx = self.binary_search(meta, 0, n - (i - 1));
            self.set_sibling_index(host, meta, idx);
        }
        if let Some(cur) = self.current_meta_save {
            self.set_container_index(cur, 0);
            host.on_index_change(cur, 0);
        }
    }

    /// `SaveSystem.StartMaybeSaveSaves` 0x96E020.
    pub fn start_maybe_save_saves(&mut self) {
        if 5 < self.state {
            self.index = 0;
            self.count = self.container.len() as i32;
            self.state = refresh_state::PROCESS_ALL_META_FILES;
        }
    }

    /// `SaveSystem.StartRefreshSaves` 0x96E060.
    pub fn start_refresh_saves(&mut self) {
        if 1 < self.state {
            self.state = refresh_state::BEGIN_REFRESH_SAVES;
        }
    }

    /// `SaveSystem.UpdateAllTextures` 0x96E1F0: every town after the first with a stale texture reloads it.
    pub fn update_all_textures(&mut self, host: &mut dyn SaveSystemHost) {
        for i in 1..self.container.len() {
            let meta = self.container[i];
            self.load_texture_if_stale(host, meta);
        }
    }

    /// `SaveSystem.SaveAndQuit(callback)` 0x96D8D0 (a coroutine) as steps: true while it should keep waiting
    /// (the host steps it each frame and calls the callback when it ends).
    pub fn save_and_quit(&mut self, host: &mut dyn SaveSystemHost) -> SaveAndQuit {
        let saved = self.maybe_save_current(host);
        let settings = self.maybe_save_settings(host, 0.0);
        SaveAndQuit { wait_current: saved, wait_settings: settings }
    }
}

/// The `SaveAndQuit` coroutine's waits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveAndQuit {
    wait_current: bool,
    wait_settings: bool,
}

impl SaveAndQuit {
    /// One frame: true while still waiting (the current town saving, then the settings).
    pub fn step(&mut self, s: &SaveSystem) -> bool {
        if self.wait_current {
            let cur = s.current_meta_save.expect("SaveAndQuit: no current");
            if s.metas[cur].is_saving() {
                return true;
            }
            self.wait_current = false;
        }
        if self.wait_settings && s.save_settings_request.is_some() {
            return true;
        }
        false
    }
}
