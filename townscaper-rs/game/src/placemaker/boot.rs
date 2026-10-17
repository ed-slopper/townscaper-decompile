//! `Placemaker.BootMaster` and `Placemaker.BootGameCore` (row E8d): how the game starts. The boot scene loads the
//! town scene ("Placemaker") and the UI scene ("FlatscreenUi") additively, finds and links the two masters, wakes
//! the UI, then the world (`OnOnEnable`, every system's `OnStart`, then `enabled`), then lets the save system boot
//! (settings, the last town or a new one). `BootGameCore` is the console sign-in: a button press signs a player
//! in, and a different (or no) user later means "signed out", which closes everything and boots the user again.
//!
//! The routines are `IEnumerator<bool>` coroutines run by `FastCoroutines.StartFastCoroutine(.., 6)`; here each is
//! a struct whose `move_next` returns `Some(current)` while it runs and `None` at its end, the shape
//! [`FastCoroutines::fast_coroutine_frame`](crate::placemaker::timing::FastCoroutines::fast_coroutine_frame) steps.
//! Scenes, objects, the UI, the save system and the platform are the host's ([`BootHost`], [`Platform`]).
//! Checked by `tests/placemaker_boot.rs` (the step order on a recording host; `_AaLineBoth` against the running
//! game with `TS_HARNESS=1`).

/// The town scene (`GetSceneByName` / `LoadSceneAsync` in `GameBootRoutine`).
pub const MAIN_SCENE: &str = "Placemaker";
/// The UI scene.
pub const UI_SCENE: &str = "FlatscreenUi";
/// `BootMaster.Start` / `OnSignedOut` / `BootGameCore.Update`: the boot coroutines' frame budget (ms).
pub const BOOT_COROUTINE_MS: i32 = 6;
/// `BootGameCore.Update`: the log line when the platform user changed.
pub const SIGNED_OUT_LOG: &str = "Signed Out!";
/// `SaveSystem.bootState` values `UserBootRoutine` acts on.
pub mod boot_state {
    /// The last town is loaded (`currentMetaSave`).
    pub const LOAD_LAST: i32 = 8;
    /// No town: a new one, and the save menu opens.
    pub const NEW_TOWN: i32 = 9;
    /// Nothing more to do but finish loading.
    pub const DONE: i32 = 10;
}

/// A scene load in flight (`AsyncOperation`), as the host numbers them.
pub type SceneOp = u32;

/// What the boot coroutines reach outside themselves: scenes, the two masters, the loading screen, the UI and the
/// save system. Each call names the game's.
pub trait BootHost {
    /// `BootMaster.instance = this`.
    fn set_instance(&mut self) {}
    /// `SceneManager.GetSceneByName(name).isLoaded`.
    fn scene_is_loaded(&mut self, name: &str) -> bool;
    /// `SceneManager.LoadSceneAsync(name, LoadSceneMode.Additive)`.
    fn load_scene_additive(&mut self, name: &str) -> SceneOp;
    /// `AsyncOperation.isDone`.
    fn operation_is_done(&mut self, op: SceneOp) -> bool;
    /// `uiMaster` from the UI scene's root objects, the last root first (`GetComponentInChildren<UiMaster>`, the
    /// first that has one). True when found.
    fn find_ui_master(&mut self) -> bool;
    /// `worldMaster` from the town scene's roots, the same way.
    fn find_world_master(&mut self) -> bool;
    /// `worldMaster.uiMaster = uiMaster; uiMaster.worldMaster = worldMaster`.
    fn link_masters(&mut self);
    /// `uiMaster.gameObject.SetActive(true)`.
    fn activate_ui_master(&mut self);
    /// `audioListener.enabled = false` (the boot scene's listener).
    fn disable_audio_listener(&mut self);
    /// `UiMaster.MaybeSetup`.
    fn ui_maybe_setup(&mut self);
    /// `worldMaster.enabled`.
    fn set_world_master_enabled(&mut self, enabled: bool);
    /// `worldMaster.gameObject.SetActive(true)`.
    fn activate_world_master(&mut self);
    /// `WorldMaster.OnOnEnable` ([`WorldMaster::on_on_enable`](crate::placemaker::world_master::WorldMaster::on_on_enable)
    /// and its components').
    fn world_on_on_enable(&mut self);
    /// `WorldMaster.OnStart` ([`WorldMaster::on_start`](crate::placemaker::world_master::WorldMaster::on_start)).
    fn world_on_start(&mut self);
    /// `WorldMaster.awaitingDim`.
    fn set_world_awaiting_dim(&mut self, value: bool);

    /// `LoadingUi.StartLoading`.
    fn loading_start(&mut self);
    /// `LoadingUi.StopLoadingNow`.
    fn loading_stop_now(&mut self);
    /// `LoadingUi.ShowLogo`.
    fn loading_show_logo(&mut self);
    /// `Dim.SetState`.
    fn dim_set_state(&mut self, state: i32);

    /// `UiMaster.UpdateVSync`.
    fn ui_update_vsync(&mut self);
    /// `UiMaster.UpdateAntialiasing`.
    fn ui_update_antialiasing(&mut self);
    /// `UiMaster.onSettingsLoaded.Invoke()`.
    fn ui_on_settings_loaded(&mut self);
    /// `SaveMenu.ResetSaveCardOrder`.
    fn save_menu_reset_card_order(&mut self);
    /// `SaveMenu.OpenWithoutRefresh`.
    fn save_menu_open_without_refresh(&mut self);
    /// `UiMaster.StartFinishLoading(instant)`.
    fn ui_start_finish_loading(&mut self, instant: bool);
    /// `UiMaster.CloseEverything`.
    fn ui_close_everything(&mut self);
    /// `UiMaster.ResetUsersUIChoises`.
    fn ui_reset_users_ui_choices(&mut self);

    /// `worldMaster.saveSystem` is there.
    fn has_save_system(&mut self) -> bool;
    /// `SaveSystem.ResetBoot`.
    fn save_reset_boot(&mut self);
    /// `SaveSystem.IterateBoot(..)`: true while it has more to do.
    fn save_iterate_boot(&mut self) -> bool;
    /// `SaveSystem.bootState` ([`boot_state`]).
    fn save_boot_state(&mut self) -> i32;
    /// `MaybeUpdateGameVersionInSettingsData`, inlined
    /// ([`WorldMaster::maybe_update_game_version_in_settings_data`](crate::placemaker::world_master::WorldMaster::maybe_update_game_version_in_settings_data)):
    /// true when the version was new.
    fn maybe_update_game_version(&mut self) -> bool;
    /// `TexturePngMaster.ResetColorAndMaterial`.
    fn texture_png_reset_color_and_material(&mut self);
    /// `WorldMaster.Load(saveSystem.currentMetaSave.saveData)`, inlined
    /// ([`WorldMaster::load_save`](crate::placemaker::world_master::WorldMaster::load_save)).
    fn world_load_current_meta_save(&mut self);
    /// `worldMaster.saveData.Clear()`, then `WorldMaster.New`'s town
    /// ([`WorldMaster::new_town`](crate::placemaker::world_master::WorldMaster::new_town), inlined, on
    /// `UnityEngine.Random`), then `WorldMaster.Load(worldMaster.saveData)`.
    fn world_new_town_and_load(&mut self);
}

/// One step of a boot coroutine: `Some(current)` while it runs, `None` at its end.
pub type Step = Option<bool>;

/// `BootMaster.<GameBootRoutine>d__13` (`MoveNext` 0x1EFDF0; `BootMaster.GameBootRoutine` 0x7C26B0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameBootRoutine {
    pub state: i32,
    pub main_scene_operation: Option<SceneOp>,
    pub flatscreen_ui_operation: Option<SceneOp>,
}

impl GameBootRoutine {
    /// `MoveNext` 0x1EFDF0: the instance; both scenes loaded (unless they are); the UI master found once the loads
    /// finish, the world master once the town scene is loaded, and linked; the UI woken; the world woken disabled;
    /// its systems started; then enabled.
    pub fn move_next(&mut self, host: &mut dyn BootHost) -> Step {
        let mut s = self.state;
        loop {
            match s {
                0 => {
                    host.set_instance();
                    self.state = 1;
                    return Some(false);
                }
                1 => {
                    self.main_scene_operation = (!host.scene_is_loaded(MAIN_SCENE)).then(|| host.load_scene_additive(MAIN_SCENE));
                    self.flatscreen_ui_operation = (!host.scene_is_loaded(UI_SCENE)).then(|| host.load_scene_additive(UI_SCENE));
                    s = 2;
                }
                2 => {
                    if self.main_scene_operation.is_some_and(|op| !host.operation_is_done(op)) {
                        self.state = 2;
                        return Some(false);
                    }
                    s = 3;
                }
                3 => {
                    if self.flatscreen_ui_operation.is_some_and(|op| !host.operation_is_done(op)) {
                        self.state = 3;
                        return Some(false);
                    }
                    host.find_ui_master();
                    s = 4;
                }
                4 => {
                    if !host.scene_is_loaded(MAIN_SCENE) {
                        self.state = 4;
                        return Some(false);
                    }
                    // without both masters the game stops on a null reference here
                    if !host.find_world_master() {
                        self.state = -1;
                        return None;
                    }
                    host.link_masters();
                    self.main_scene_operation = None;
                    self.flatscreen_ui_operation = None;
                    self.state = 5;
                    return Some(true);
                }
                5 => {
                    host.activate_ui_master();
                    host.disable_audio_listener();
                    host.ui_maybe_setup();
                    self.state = 6;
                    return Some(true);
                }
                6 => {
                    host.set_world_master_enabled(false);
                    host.activate_world_master();
                    host.world_on_on_enable();
                    self.state = 7;
                    return Some(true);
                }
                7 => {
                    host.world_on_start();
                    self.state = 8;
                    return Some(true);
                }
                8 => {
                    host.set_world_master_enabled(true);
                    self.state = 9;
                    return Some(true);
                }
                _ => {
                    self.state = -1;
                    return None;
                }
            }
        }
    }
}

/// `BootMaster.<UserBootRoutine>d__14` (`MoveNext` 0x1F2FE0; `BootMaster.UserBootRoutine` 0x7C29F0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserBootRoutine {
    pub state: i32,
}

impl UserBootRoutine {
    /// `MoveNext` 0x1F2FE0: the save system boots (`ResetBoot`, then `IterateBoot` once a frame while it has more
    /// to do); then vsync, antialiasing, `onSettingsLoaded`, the version check (a new version also resets the
    /// texture master's colour), the save cards' order; then by `bootState`: the last town loads, or a new town
    /// is made and the save menu opens, and the loading screen finishes.
    pub fn move_next(&mut self, host: &mut dyn BootHost) -> Step {
        match self.state {
            0 => {
                self.state = -1;
                let has = host.has_save_system();
                if has {
                    host.save_reset_boot();
                    if host.save_iterate_boot() {
                        self.state = 1;
                        return Some(false);
                    }
                }
            }
            1 => {
                self.state = -1;
                if host.save_iterate_boot() {
                    self.state = 1;
                    return Some(false);
                }
            }
            _ => return None,
        }
        host.ui_update_vsync();
        host.ui_update_antialiasing();
        host.ui_on_settings_loaded();
        if host.maybe_update_game_version() {
            host.texture_png_reset_color_and_material();
        }
        host.save_menu_reset_card_order();
        match host.save_boot_state() {
            boot_state::LOAD_LAST => {
                host.world_load_current_meta_save();
                host.set_world_awaiting_dim(false);
                host.ui_start_finish_loading(false);
            }
            boot_state::DONE => host.ui_start_finish_loading(false),
            boot_state::NEW_TOWN => {
                host.world_new_town_and_load();
                host.loading_stop_now();
                host.save_menu_open_without_refresh();
                host.set_world_awaiting_dim(false);
            }
            _ => {}
        }
        None
    }
}

/// A routine that runs `inner` to its end, passing its values on (`yield return inner.Current` while
/// `inner.MoveNext()`), as `FirstBootWrapper` and `SignedOutBootWrapper` do.
fn forward(inner: &mut UserBootRoutine, host: &mut dyn BootHost) -> Step {
    inner.move_next(host)
}

/// `BootMaster.<FirstBootWrapper>d__12` (`MoveNext` 0x1EFC40; `BootMaster.FirstBootWrapper` 0x7C2660).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstBootWrapper {
    pub state: i32,
    pub game_boot_routine: GameBootRoutine,
    pub user_boot_routine: Option<UserBootRoutine>,
}

impl FirstBootWrapper {
    /// `MoveNext` 0x1EFC40: the loading screen, then [`GameBootRoutine`] to its end, then (in the same step)
    /// [`UserBootRoutine`] to its end.
    pub fn move_next(&mut self, host: &mut dyn BootHost) -> Step {
        match self.state {
            0 => {
                host.loading_start();
                self.game_boot_routine = GameBootRoutine::default();
                self.state = 1;
            }
            1 | 2 => {}
            _ => return None,
        }
        if self.state == 1 {
            if let Some(v) = self.game_boot_routine.move_next(host) {
                return Some(v);
            }
            self.user_boot_routine = Some(UserBootRoutine::default());
            self.state = 2;
        }
        match self.user_boot_routine.as_mut().and_then(|u| forward(u, host)) {
            Some(v) => Some(v),
            None => {
                self.state = -1;
                None
            }
        }
    }
}

/// `BootMaster.<SignedOutBootWrapper>d__16` (`MoveNext` 0x1F0EB0; `BootMaster.SignedOutBootWrapper` 0x7C2900).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignedOutBootWrapper {
    pub state: i32,
    pub user_boot_routine: Option<UserBootRoutine>,
}

impl SignedOutBootWrapper {
    /// `MoveNext` 0x1F0EB0: every window closed and the user's choices reset, a new sign-in
    /// ([`BootGameCore::start_new_sign_in`]), the logo, the dim (state 2); one step a frame until the core is
    /// signed in (state 2); then the loading screen and [`UserBootRoutine`] to its end.
    pub fn move_next(&mut self, host: &mut dyn BootHost, core: &mut BootGameCore, platform: &mut dyn Platform) -> Step {
        match self.state {
            0 => {
                host.ui_close_everything();
                host.ui_reset_users_ui_choices();
                core.start_new_sign_in(platform);
                host.loading_show_logo();
                host.dim_set_state(2);
                self.state = 1;
            }
            1 | 2 => {}
            _ => return None,
        }
        if self.state == 1 {
            if core.state != 2 {
                return Some(false);
            }
            host.loading_start();
            self.user_boot_routine = Some(UserBootRoutine::default());
            self.state = 2;
        }
        match self.user_boot_routine.as_mut().and_then(|u| forward(u, host)) {
            Some(v) => Some(v),
            None => {
                self.state = -1;
                None
            }
        }
    }
}

/// What `BootMaster`'s `Start` / `OnSignedOut` hand to `FastCoroutines.StartFastCoroutine` (6 ms).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootCoroutine {
    First(FirstBootWrapper),
    SignedOut(SignedOutBootWrapper),
}

/// `Placemaker.BootMaster` (`.ctor` 0x1F53D0, `Awake` 0x1D4430: nothing).
#[derive(Clone, Debug, Default)]
pub struct BootMaster {
    /// The `Shader.SetGlobalVector("_AaLineBoth", ..)` value `OnEnable` set last.
    pub aa_line_both: Option<[f32; 4]>,
}

/// What `BootMaster.OnEnable` reaches: the screen scale and the global shader state.
pub trait BootScreen {
    /// `UiScaler.GetScale`.
    fn ui_scale(&mut self) -> f32;
    /// `UiScaler.GetDefaultAntiAliasing`.
    fn default_anti_aliasing(&mut self) -> i32;
    /// `QualitySettings.antiAliasing = value`.
    fn set_anti_aliasing(&mut self, value: i32);
    /// `Shader.SetGlobalVector(name, value)`.
    fn set_global_vector(&mut self, name: &str, value: [f32; 4]);
}

/// `Mathf.Clamp(float, float, float)`.
fn clamp(v: f32, min: f32, max: f32) -> f32 {
    if v < min {
        min
    } else if v > max {
        max
    } else {
        v
    }
}

/// `BootMaster.OnEnable`'s `_AaLineBoth`: `(1, s, s, s)` with `s = Clamp(UiScaler.GetScale() * 0.7f, 1, 2)`.
pub fn aa_line_both(ui_scale: f32) -> [f32; 4] {
    let s = clamp(ui_scale * 0.7, 1.0, 2.0);
    [1.0, s, s, s]
}

impl BootMaster {
    /// `Placemaker.BootMaster.OnEnable` 0x7C2700: the instance; `UiMaster.MaybeSetup` (when there is one); the
    /// default antialiasing; `_AaLineBoth` ([`aa_line_both`]); `WorldMaster.OnOnEnable` (when there is one).
    pub fn on_enable(&mut self, host: &mut dyn BootHost, screen: &mut dyn BootScreen, has_ui_master: bool, has_world_master: bool) {
        host.set_instance();
        if has_ui_master {
            host.ui_maybe_setup();
        }
        let aa = screen.default_anti_aliasing();
        screen.set_anti_aliasing(aa);
        let v = aa_line_both(screen.ui_scale());
        screen.set_global_vector(crate::placemaker::timing::shader_ids::AA_LINE_BOTH, v);
        self.aa_line_both = Some(v);
        if has_world_master {
            host.world_on_on_enable();
        }
    }

    /// `Placemaker.BootMaster.Start` 0x7C2950: [`FirstBootWrapper`] as a fast coroutine
    /// ([`BOOT_COROUTINE_MS`]).
    pub fn start(&mut self) -> BootCoroutine {
        BootCoroutine::First(FirstBootWrapper::default())
    }

    /// `Placemaker.BootMaster.OnSignedOut` 0x7C2860: [`SignedOutBootWrapper`] as a fast coroutine.
    pub fn on_signed_out(&mut self) -> BootCoroutine {
        BootCoroutine::SignedOut(SignedOutBootWrapper::default())
    }
}

/// `Screen.SetResolution` arguments `BootMaster.DoFullscreen` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    /// `Screen.SetResolution(width, height, true)`.
    Resolution { width: i32, height: i32 },
    /// `Screen.fullScreen = true` (no resolutions listed).
    Toggle,
}

/// `Placemaker.BootMaster.DoFullscreen` 0x7C25A0: on platform 2 (`Application.platform`, `WindowsPlayer`) the largest
/// possible size (`int.MaxValue` both ways); else the last of `Screen.resolutions`, or just fullscreen when
/// there are none.
pub fn do_fullscreen(platform: i32, resolutions: &[(i32, i32)]) -> Fullscreen {
    if platform == 2 {
        return Fullscreen::Resolution { width: i32::MAX, height: i32::MAX };
    }
    match resolutions.last() {
        Some(&(width, height)) => Fullscreen::Resolution { width, height },
        None => Fullscreen::Toggle,
    }
}

/// A sign-in request in flight (`IAsyncRequest`), as the platform numbers them.
pub type SignInRequest = u32;
/// `IAsyncRequest.Status` when it succeeded.
pub const REQUEST_SUCCEEDED: i32 = 2;
/// `IAsyncRequest.Status` when it failed.
pub const REQUEST_FAILED: i32 = 4;

/// The platform SDK as `BootGameCore` uses it (FurySDK's `Platform.SDK`, Rewired's players, `PlatformManager`).
pub trait Platform {
    /// `Platform.SDK != null`.
    fn has_sdk(&mut self) -> bool;
    /// `ReInput.players.GetPlayers(false)`, each player's `GetAnyButtonDown()`.
    fn players_any_button_down(&mut self) -> Vec<bool>;
    /// `Platform.SDK.SignIn(mode)` with `BootGameCore_OnComplete` on its completion.
    fn sign_in(&mut self, mode: i32) -> SignInRequest;
    /// `IAsyncRequest.Status`.
    fn request_status(&mut self, request: SignInRequest) -> i32;
    /// `Platform.SDK.User?.Id`.
    fn user_id(&mut self) -> Option<i64>;
    /// `PlatformManager`'s signed-in user id (a static `long?`).
    fn signed_in_user(&mut self) -> Option<i64>;
    fn set_signed_in_user(&mut self, id: Option<i64>);
    /// `StartNewSignIn`'s tail: the system language looked up in `Localization`'s table and, when found, made
    /// `LocalizationManager.CurrentLanguage`.
    fn apply_system_language(&mut self) {}
    /// `Debug.Log`.
    fn log(&mut self, _text: &str) {}
}

/// `Placemaker.BootGameCore`: the console sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootGameCore {
    /// 1: waiting for a player to press a button and sign in; 2: signed in; 3: signed out.
    pub state: i32,
    pub user_prompted: bool,
    pub sign_in_failed_once: bool,
    /// `_selectedPlayer`, by its place in `GetPlayers`.
    pub selected_player: Option<usize>,
    pub sign_in_request: Option<SignInRequest>,
    /// `promptState.targetBool`: the "press a button" prompt shows (the `UpdateState` itself is the UI's, G7;
    /// `OnEnable` 0x7C1E30 subscribes it to `UiMaster`'s update).
    pub prompt: bool,
    /// The class's static first-sign-in flag (`.cctor` 0x7C2510: true): the first sign-in asks with mode 3.
    pub first_sign_in: bool,
}

impl Default for BootGameCore {
    /// `.ctor` 0x7C2540 and `.cctor` 0x7C2510.
    fn default() -> Self {
        BootGameCore {
            state: 0,
            user_prompted: false,
            sign_in_failed_once: false,
            selected_player: None,
            sign_in_request: None,
            prompt: false,
            first_sign_in: true,
        }
    }
}

impl BootGameCore {
    /// `Placemaker.BootGameCore.StartNewSignIn` 0x7C1EA0: back to waiting (state 1), nothing selected or asked,
    /// the prompt on, the system language.
    pub fn start_new_sign_in(&mut self, platform: &mut dyn Platform) {
        self.state = 1;
        self.selected_player = None;
        self.user_prompted = false;
        self.sign_in_failed_once = false;
        self.sign_in_request = None;
        self.prompt = true;
        platform.apply_system_language();
    }

    /// `Placemaker.BootGameCore.Update` 0x7C1FA0. Waiting (1), with an SDK and no player yet: the first player to
    /// press a button (while nobody was asked) is asked to sign in — mode 2, or 3 on the very first sign-in
    /// unless one failed. Signed in (2), with a user stored: when the SDK's user is gone or another, state 3,
    /// "Signed Out!", and true: the boot master must start [`SignedOutBootWrapper`]
    /// ([`BootMaster::on_signed_out`]'s coroutine, 6 ms).
    pub fn update(&mut self, platform: &mut dyn Platform) -> bool {
        match self.state {
            1 => {
                if !platform.has_sdk() || self.selected_player.is_some() {
                    return false;
                }
                for (i, down) in platform.players_any_button_down().into_iter().enumerate() {
                    if down && !self.user_prompted {
                        self.selected_player = Some(i);
                        self.user_prompted = true;
                        let mode = if !self.sign_in_failed_once && self.first_sign_in { 3 } else { 2 };
                        self.sign_in_request = Some(platform.sign_in(mode));
                        self.first_sign_in = false;
                    }
                }
                false
            }
            2 => {
                let Some(stored) = platform.signed_in_user() else { return false };
                if platform.user_id() == Some(stored) {
                    return false;
                }
                self.state = 3;
                platform.log(SIGNED_OUT_LOG);
                true
            }
            _ => false,
        }
    }

    /// `Placemaker.BootGameCore.BootGameCore_OnComplete` 0x7C1C80: a sign-in that succeeded stores the user and
    /// signs in (state 2, the prompt off); one that failed is remembered; either way the request, the player and
    /// the asking are cleared.
    pub fn on_complete(&mut self, platform: &mut dyn Platform) {
        let Some(req) = self.sign_in_request else { return };
        match platform.request_status(req) {
            REQUEST_SUCCEEDED => {
                if let Some(id) = platform.user_id() {
                    platform.set_signed_in_user(Some(id));
                    self.state = 2;
                    self.prompt = false;
                }
            }
            REQUEST_FAILED => self.sign_in_failed_once = true,
            _ => {}
        }
        self.user_prompted = false;
        self.sign_in_request = None;
        self.selected_player = None;
    }
}
