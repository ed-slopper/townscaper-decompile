//! `Placemaker.Ui.MasterClicker` (row G1c): the pointer over the town. It hovers each frame (`SetHover` at the
//! pointer), turns clicks into the maker's add (left), remove (right, or a left press held past 0.25 s) and
//! colour pick (middle, or a press held 1.5 s), bobs the pressed voxel while a press lasts, and turns drags into
//! the orbital camera's rotation (left, with a fling on release) or pan (right / middle). Unity's EventSystem
//! raises the pointer events (`OnPointerDown` … `OnEndDrag`); the host raises them for us (pointer ids: -1 left,
//! -2 right, -3 middle, 0 and up touches). `GroundClicker`, its older copy, is in no scene and not ported.

use crate::os::hex;
use crate::placemaker::graphs::{Graph, GraphHost, VoxelId, VoxelType};
use crate::placemaker::grid_generator::GridGenerator;
use crate::placemaker::hover_data::HoverData;
use crate::placemaker::maker::{Maker, MakerHost};
use crate::unity::mathematics::{f2, Float2};
use crate::unity::mathf;
use crate::unity::vector::{Vector2, Vector3};

/// `UnityEngine.EventSystems.PointerEventData`: what a pointer event carries.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PointerEvent {
    /// -1 left, -2 right, -3 middle; touches from 0.
    pub pointer_id: i32,
    pub position: Vector2,
    pub delta: Vector2,
}

/// What the clicker reaches besides the maker, the hover and the graph.
pub trait ClickerHost {
    fn unscaled_time(&self) -> f32;
    /// `Time.time` (the drag ring's stamps).
    fn time(&self) -> f32;
    fn frame_count(&self) -> i32;
    fn mouse_present(&self) -> bool {
        true
    }
    fn mouse_position(&self) -> Vector2;
    /// `Screen.width`, `Screen.height`.
    fn screen(&self) -> (i32, i32);
    /// `WorldMaster.expTFaster`.
    fn exp_t_faster(&self) -> f32;
    /// `inputModeManager.touchAndMouseState.targetBool` (`None` without an input mode manager).
    fn touch_and_mouse_state(&self) -> Option<bool> {
        Some(true)
    }
    // the orbital camera (row G2)
    fn camera_stop_rotation(&mut self) {}
    fn camera_rotate(&mut self, _by: Float2) {}
    fn camera_pan(&mut self, _position: Vector2, _delta: Vector2) {}
    fn camera_set_rotation_speed(&mut self, _speed: Float2) {}
    fn camera_scroll_zoom(&mut self, _amount: f32) {}
    /// `Input.GetAxis("Mouse ScrollWheel")`.
    fn scroll_axis(&self) -> f32 {
        0.0
    }
    // the voxel bob effect (row G1d)
    fn bob_set_pressed(&mut self, _position: Vector3) {}
    fn bob_stop_pressing(&mut self, _velocity: f32) {}
    fn bob_stop_pressing_no_velocity(&mut self) {}
    /// `voxelBobEffect.bobCurrent` (`None` without the effect).
    fn bob_current(&self) -> Option<f32> {
        None
    }
    /// Sets `bobCurrent` and runs `UpdateShader`.
    fn bob_set_current(&mut self, _bob: f32) {}
    // the click effect (row G1d)
    fn click_effect(&mut self, _hover: &HoverData, _add: bool, _voxel: Option<VoxelId>) {}
    fn click_effect_color_pick(&mut self, _hover: &HoverData) {}
    /// `master.paletteMenu.SelectPicker(type)` (row G7c).
    fn palette_select_picker(&mut self, _voxel_type: i32) {}
    /// The press sound (`pressSource.Play` / `Stop`).
    fn press_sound(&mut self, _play: bool) {}
    /// `master.worldMaster.hoverHightlight.Draw()` (row G1d).
    fn hover_highlight_draw(&mut self) {}
}

/// What a click acts on: the world master's graph, grid, maker and hover.
pub struct World<'a> {
    pub graph: &'a mut Graph,
    pub grid: &'a mut GridGenerator,
    pub graph_host: &'a mut dyn GraphHost,
    pub maker: &'a mut Maker,
    pub maker_host: &'a mut dyn MakerHost,
    pub hover: &'a mut HoverData,
}

/// The clicker's state.
#[derive(Clone, Debug, PartialEq)]
pub struct MasterClicker {
    pub enabled: bool,
    pub current_voxel_type: VoxelType,
    pub hovered: bool,
    pub bobbing: bool,
    pub about_to_remove: bool,
    pub dragging: bool,
    pub pointer_pressed: bool,
    pub pointer_press_used: bool,
    pub pointer_press_time: f32,
    pub pointer_position: Vector2,
    pub click_frame: i32,
    pub hover_frame: i32,
    /// The last five drags: `(x, y, Time.time)`.
    pub drags: [Vector3; 5],
    pub drag_index: i32,
    pub touch_location_threshold: f32,
}

impl Default for MasterClicker {
    /// `Placemaker.Ui.MasterClicker..ctor` 0x32C6C0: a five-slot drag ring.
    fn default() -> MasterClicker {
        MasterClicker {
            enabled: true,
            current_voxel_type: VoxelType::from_i8(0),
            hovered: false,
            bobbing: false,
            about_to_remove: false,
            dragging: false,
            pointer_pressed: false,
            pointer_press_used: false,
            pointer_press_time: 0.0,
            pointer_position: Vector2 { x: 0.0, y: 0.0 },
            click_frame: 0,
            hover_frame: 0,
            drags: [Vector3::ZERO; 5],
            drag_index: 0,
            touch_location_threshold: 0.0,
        }
    }
}

impl MasterClicker {
    /// `IUiSetup.OnSetup` 0x32B830: `touchLocationThreshold = |screen| · 0.022`; the menu-state subscription
    /// is [`on_menu_state`](Self::on_menu_state).
    pub fn on_setup(&mut self, host: &dyn ClickerHost) {
        let (w, h) = host.screen();
        let s = Vector2 { x: w as f32, y: h as f32 };
        self.touch_location_threshold = s.magnitude() * 0.022;
    }

    /// The `OnSetup` lambda 0x32BD10: enabled only in menu state 0x20 or 0x22 (`(state & ~2) == 0x20`).
    pub fn on_menu_state(&mut self, state: i32) {
        self.enabled = (state & !2) == 0x20;
    }

    /// `IPointerEnterHandler.OnPointerEnter` 0x32C3A0: the mouse (with one present) hovers.
    pub fn on_pointer_enter(&mut self, host: &dyn ClickerHost, e: &PointerEvent) {
        if e.pointer_id == -1 && host.mouse_present() {
            self.hovered = true;
        }
    }

    /// `IPointerExitHandler.OnPointerExit` 0x32C3D0: the mouse leaves: no hover.
    pub fn on_pointer_exit(&mut self, host: &dyn ClickerHost, hover: &mut HoverData, e: &PointerEvent) {
        if e.pointer_id != -1 || !host.mouse_present() {
            return;
        }
        self.hovered = false;
        hover.clear_hover();
    }

    /// `MasterClicker.MaybeUpdateHover` 0x32B5B0: once a frame, `SetHover(position, maker.actionId)`.
    pub fn maybe_update_hover(&mut self, host: &dyn ClickerHost, w: &mut World, set_hover: &mut dyn FnMut(&mut HoverData, &mut GridGenerator, &Graph, Vector2, i32), position: Vector2) {
        let frame = host.frame_count();
        if frame == self.hover_frame {
            return;
        }
        self.hover_frame = frame;
        set_hover(w.hover, w.grid, w.graph, position, w.maker.action_id);
    }

    /// `MasterClicker.Update` 0x32C490: the pointer position from the mouse; while bobbing, a press held 1.5 s
    /// picks the colour (stopping the bob when nothing was picked) and is used up; past 0.35 s it is about to
    /// remove (the press sound) and the bob sinks: `Lerp(-0.2, -0.4, 1 - (1 - t)²)` with
    /// `t = InverseLerp(0.32, 1.5, held)`, eased into `bobCurrent` by `expTFaster`.
    pub fn update(&mut self, host: &mut dyn ClickerHost, w: &mut World) {
        if host.mouse_present() {
            self.pointer_position = host.mouse_position();
        }
        if !self.bobbing {
            return;
        }
        let held = host.unscaled_time() - self.pointer_press_time;
        if 1.5 <= held {
            let picked = self.color_pick(host, w.hover, w.graph);
            self.bobbing = false;
            if !picked {
                host.bob_stop_pressing(0.0);
            }
            self.pointer_press_used = true;
        } else if 0.35 < held {
            if !self.about_to_remove {
                self.about_to_remove = true;
                host.press_sound(true);
            }
            let t = mathf::inverse_lerp(0.32, 1.5, held);
            let target = mathf::lerp(-0.2, -0.4, 1.0 - (1.0 - t) * (1.0 - t));
            if let Some(current) = host.bob_current() {
                let b = mathf::lerp(target, current, host.exp_t_faster());
                host.bob_set_current(b);
            }
        }
    }

    /// `MasterClicker.LateUpdate` 0x32B4B0: while the mouse hovers and nothing drags, the hover follows the
    /// pointer (unless a click happened this frame or it is pressed); with the touch-and-mouse input mode (or
    /// no input mode manager) the hover highlight is drawn.
    pub fn late_update(&mut self, host: &mut dyn ClickerHost, w: &mut World, set_hover: &mut dyn FnMut(&mut HoverData, &mut GridGenerator, &Graph, Vector2, i32)) {
        if !self.hovered || self.dragging {
            return;
        }
        if host.frame_count() != self.click_frame && !self.pointer_pressed {
            let p = self.pointer_position;
            self.maybe_update_hover(host, w, set_hover, p);
        }
        if host.touch_and_mouse_state() == Some(false) {
            return;
        }
        host.hover_highlight_draw();
    }

    /// `MasterClicker.OnGUI` 0x32B620: while hovered, the scroll wheel zooms the camera.
    pub fn on_gui(&mut self, host: &mut dyn ClickerHost) {
        if !self.hovered {
            return;
        }
        let a = host.scroll_axis();
        if a != 0.0 {
            host.camera_scroll_zoom(a);
        }
    }

    /// `IPointerDownHandler.OnPointerDown` 0x32C260 (mouse buttons only): hover at the press, pressed and not
    /// used, the press time; on a voxel the bob starts at the source position.
    pub fn on_pointer_down(&mut self, host: &mut dyn ClickerHost, w: &mut World, set_hover: &mut dyn FnMut(&mut HoverData, &mut GridGenerator, &Graph, Vector2, i32), e: &PointerEvent) {
        if -1 < e.pointer_id {
            return;
        }
        self.maybe_update_hover(host, w, set_hover, e.position);
        self.pointer_pressed = true;
        self.pointer_press_used = false;
        self.pointer_press_time = host.unscaled_time();
        if w.hover.voxel.is_none() {
            return;
        }
        host.bob_set_pressed(w.hover.get_src_pos());
        self.bobbing = true;
    }

    /// `IPointerUpHandler.OnPointerUp` 0x32C420 (mouse buttons only): released, the bob and the press sound
    /// stop.
    pub fn on_pointer_up(&mut self, host: &mut dyn ClickerHost, e: &PointerEvent) {
        if -1 < e.pointer_id {
            return;
        }
        self.pointer_pressed = false;
        self.bobbing = false;
        if self.about_to_remove {
            self.about_to_remove = false;
            host.press_sound(false);
        }
        host.bob_stop_pressing(0.0);
    }

    /// `IPointerClickHandler.OnPointerClick` 0x32C170 (mouse buttons, not while dragging, once a frame, unless
    /// the press was used): left adds (removes when held past 0.25 s), right removes, middle picks the colour.
    /// Returns whether the event was used.
    pub fn on_pointer_click(&mut self, host: &mut dyn ClickerHost, w: &mut World, e: &PointerEvent) -> bool {
        if !(e.pointer_id < 0 && !self.dragging) {
            return false;
        }
        let frame = host.frame_count();
        if self.click_frame == frame {
            return false;
        }
        self.click_frame = frame;
        if self.pointer_press_used {
            return false;
        }
        match e.pointer_id {
            -1 => {
                if self.pointer_press_time + 0.25 < host.unscaled_time() {
                    self.remove_click(host, w);
                } else {
                    self.add_click(host, w);
                }
                true
            }
            -2 => {
                self.remove_click(host, w);
                true
            }
            -3 => {
                self.color_pick(host, w.hover, w.graph);
                true
            }
            _ => false,
        }
    }

    /// `IBeginDragHandler.OnBeginDrag` 0x32BD40 (mouse buttons): the camera stops turning, the bob stops,
    /// dragging; the event is used and the hover cleared.
    pub fn on_begin_drag(&mut self, host: &mut dyn ClickerHost, hover: &mut HoverData, e: &PointerEvent) {
        if -1 < e.pointer_id {
            return;
        }
        host.camera_stop_rotation();
        host.bob_stop_pressing(0.0);
        self.bobbing = false;
        self.dragging = true;
        hover.clear_hover();
    }

    /// `IDragHandler.OnDrag` 0x32BDF0: a left drag turns the camera by `delta / min(width, height) · 130`
    /// (y negated) and records it in the drag ring with `Time.time`; a right or middle drag pans.
    pub fn on_drag(&mut self, host: &mut dyn ClickerHost, e: &PointerEvent) {
        match e.pointer_id {
            -1 => {
                let (sw, sh) = host.screen();
                let m = if sh < sw { sh } else { sw } as f32;
                let d = hex::get_hex2_f(crate::unity::mathematics::f3(e.delta.x, e.delta.y, 0.0));
                let x = (d.x / m) * 130.0;
                let y = (d.y / m) * 130.0 * -1.0;
                host.camera_rotate(f2(x, y));
                self.drag_index = (self.drag_index + 1) % 5;
                self.drags[self.drag_index as usize] = Vector3 { x, y, z: host.time() };
            }
            -2 | -3 => host.camera_pan(e.position, e.delta),
            _ => {}
        }
    }

    /// `IEndDragHandler.OnEndDrag` 0x32BFE0 (mouse buttons, while dragging): the fling: the drags of the last
    /// 0.2 s (newest first) summed and divided by the oldest one's age, as the camera's rotation speed.
    pub fn on_end_drag(&mut self, host: &mut dyn ClickerHost, e: &PointerEvent) {
        if !(e.pointer_id < 0 && self.dragging) {
            return;
        }
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        let mut age = -1.0f32;
        for i in 0..5 {
            let k = ((self.drag_index - i) + 5) % 5;
            let d = self.drags[k as usize];
            let a = host.time() - d.z;
            if 0.2 <= a {
                break;
            }
            sx = d.x + sx;
            sy = d.y + sy;
            age = mathf::max(a, age);
        }
        if 0.0 < age {
            host.camera_set_rotation_speed(f2(sx / age, sy / age));
        }
        self.dragging = false;
    }

    /// `MasterClicker.TouchDown(position)` 0x32BB00: hover at the touch; on a voxel the bob starts; not used,
    /// the press time.
    pub fn touch_down(&mut self, host: &mut dyn ClickerHost, w: &mut World, set_hover: &mut dyn FnMut(&mut HoverData, &mut GridGenerator, &Graph, Vector2, i32), position: Vector2) {
        set_hover(w.hover, w.grid, w.graph, position, w.maker.action_id);
        if w.hover.voxel.is_some() {
            host.bob_set_pressed(w.hover.get_src_pos());
            self.bobbing = true;
        }
        self.pointer_press_used = false;
        self.pointer_press_time = host.unscaled_time();
    }

    /// `MasterClicker.TouchUp(couldAct)` 0x32BC50: an unused press adds, or removes when held past 0.25 s
    /// (if removal is valid); the bob stops without velocity, the press sound stops.
    pub fn touch_up(&mut self, host: &mut dyn ClickerHost, w: &mut World, could_act: bool) {
        if could_act && !self.pointer_press_used {
            if self.pointer_press_time + 0.25 < host.unscaled_time() {
                if w.hover.valid_remove {
                    self.remove_click(host, w);
                }
            } else {
                self.add_click(host, w);
            }
        }
        host.bob_stop_pressing_no_velocity();
        self.bobbing = false;
        if self.about_to_remove {
            self.about_to_remove = false;
            host.press_sound(false);
        }
    }

    /// `MasterClicker.AddClick` 0x32AB40: with a valid add and no drag, the maker's add (inlined
    /// `Maker.AddClick`) with the current type, then the click effect on the new voxel.
    pub fn add_click(&mut self, host: &mut dyn ClickerHost, w: &mut World) -> Option<VoxelId> {
        if !w.hover.valid_add || self.dragging {
            return None;
        }
        let hover = w.hover.clone();
        let v = w.maker.add_click(w.graph, w.grid, w.graph_host, w.maker_host, &hover, self.current_voxel_type);
        if v.is_some() {
            host.click_effect(w.hover, true, v);
        }
        v
    }

    /// `MasterClicker.RemoveClick` 0x32B920: with a valid remove and no drag, the click effect on the hovered
    /// voxel first, then the maker's remove (inlined `Maker.RemoveClick`).
    pub fn remove_click(&mut self, host: &mut dyn ClickerHost, w: &mut World) -> bool {
        if !w.hover.valid_remove || self.dragging {
            return false;
        }
        host.click_effect(w.hover, false, w.hover.voxel);
        let hover = w.hover.clone();
        w.maker.remove_click(w.graph, w.graph_host, w.maker_host, &hover);
        true
    }

    /// `MasterClicker.ColorPick` 0x32B380: on a voxel of a colour (type 0..=14), the palette picks it and the
    /// click effect shows it.
    pub fn color_pick(&mut self, host: &mut dyn ClickerHost, hover: &HoverData, graph: &Graph) -> bool {
        let Some(v) = hover.voxel else { return false };
        let t = graph.voxel(v).voxel_type as i8;
        if 0x0E < t {
            return false;
        }
        host.palette_select_picker(t as i32);
        host.click_effect_color_pick(hover);
        true
    }
}
