//! `Placemaker.Maker` (rows E4d, G1b): the edits the player makes and their undo list. A click becomes a new
//! action id (`BeginNewAction`: the redo tail dropped, the list trimmed to 998 actions, the world reset and the
//! collapse's propagation counter stepped) and one `Action` per voxel changed (`AddAction`), applied to the graph
//! at once. `Undo` / `Redo` replay one action id backwards / forwards through `Graph.ApplyAction` (with the click
//! effect). Checked against a fresh game `Maker` on the graph tests' stand-in town in
//! `tests/placemaker_maker.rs` (`TS_HARNESS=1`).

use crate::placemaker::graphs::{Action, Graph, GraphHost, VoxelId, VoxelType};
use crate::placemaker::grid_generator::GridGenerator;
use crate::placemaker::hover_data::HoverData;
use crate::unity::mathematics::Int2;
use crate::unity::vector::{v3, Vector3};

/// The most action ids the undo list keeps beyond the current one (`actionCount > 998` trims).
pub const MAX_ACTION_COUNT: i32 = 998;

/// The UI sounds the maker plays (`undoClip`, `redoClip`, through `master.uiAudioSourcePool`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MakerClip {
    Undo,
    Redo,
}

/// What the maker reaches outside the graph.
pub trait MakerHost {
    /// `master.StartCounting()` (`UniversalStopwatch.StartCounting`: the frame budget restarts).
    fn start_counting(&mut self) {}
    /// `master.ResetState()`.
    fn reset_state(&mut self) {}
    /// `master.SetDirty()`.
    fn set_dirty(&mut self) {}
    /// `master.hoverData.ClearHover()`.
    fn clear_hover(&mut self) {}
    /// `master.uiAudioSourcePool.Play(clip, true)`.
    fn play_ui(&mut self, _clip: MakerClip) {}
    /// `rapidButton.Press(1)` (row G1c): whether a held undo / redo fires this frame.
    fn rapid_button_press(&mut self) -> bool {
        true
    }
}

/// A host that does nothing.
pub struct NoMakerHost;
impl MakerHost for NoMakerHost {}

/// `Placemaker.Maker`.
#[derive(Clone, Debug, PartialEq)]
pub struct Maker {
    pub actions: Vec<Action>,
    pub action_index: i32,
    pub action_count: i32,
    pub action_redo_max: i32,
    pub action_id: i32,
    /// The `onQueueChange` events raised, as (actionCount, actionRedoMax - actionCount) (the undo / redo
    /// buttons listen to them).
    pub queue_changes: Vec<(i32, i32)>,
    pub last_added_voxel: Option<VoxelId>,
    pub last_action_pos: Vector3,
}

impl Default for Maker {
    /// `Placemaker.Maker..ctor` 0x329BB0: an empty list, `lastActionPos = (0, -10000, 0)`, a no-op
    /// `onQueueChange`; the rapid button from `RapidButton`'s defaults (row G1c).
    fn default() -> Maker {
        Maker {
            actions: vec![],
            action_index: 0,
            action_count: 0,
            action_redo_max: 0,
            action_id: 0,
            queue_changes: vec![],
            last_added_voxel: None,
            last_action_pos: v3(0.0, -10000.0, 0.0),
        }
    }
}

impl Maker {
    /// `Placemaker.Maker.AddAction` 0x328D60: a new action with the current `actionId`, appended to `actions`,
    /// `actionIndex++`.
    pub fn add_action(&mut self, hex_pos: Int2, height: u8, in_type: VoxelType, out_type: VoxelType) -> Action {
        let a = Action::new(hex_pos, height, in_type, out_type, self.action_id);
        self.actions.push(a);
        self.action_index += 1;
        a
    }

    /// `Placemaker.Maker.ClearUndoQueue` 0x3291D0: no actions, every counter 0, the queue-change event.
    pub fn clear_undo_queue(&mut self) {
        self.actions.clear();
        self.action_index = 0;
        self.action_count = 0;
        self.action_redo_max = 0;
        self.action_queue_changed();
    }

    /// `Placemaker.Maker.ActionQueueChanged` 0x328D10: raises `onQueueChange(actionCount, actionRedoMax -
    /// actionCount)`.
    pub fn action_queue_changed(&mut self) {
        self.queue_changes.push((self.action_count, self.action_redo_max - self.action_count));
    }

    /// The first `TrimActionList` / `BeginNewAction` step: while more than 998 action ids are kept, the oldest
    /// id's actions go (`actionIndex` and `actionCount` lowered); then everything past `actionIndex` (the redo
    /// tail).
    fn trim(&mut self) {
        while self.action_count > MAX_ACTION_COUNT {
            let first = self.actions.first().expect("Maker: ArgumentOutOfRangeException").action_id;
            let n = self.actions.iter().take_while(|a| a.action_id == first).count();
            self.actions.drain(0..n);
            self.action_index -= n as i32;
            self.action_count -= 1;
        }
        let keep = self.action_index as usize;
        self.actions.truncate(keep);
    }

    /// `Placemaker.Maker.TrimActionList` 0x3298C0.
    pub fn trim_action_list(&mut self) {
        self.trim();
    }

    /// `Placemaker.Maker.BeginNewAction` 0x328F90: the list trimmed ([`trim`](Self::trim)), the world reset,
    /// the collapse's propagation counter stepped; `actionCount++`, `actionId++`, nothing to redo; the
    /// queue-change event.
    pub fn begin_new_action(&mut self, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) {
        self.trim();
        host.reset_state();
        graph_host.wfc_iterate_propagation_count();
        self.action_count += 1;
        self.action_id += 1;
        self.action_redo_max = self.action_count;
        self.action_queue_changed();
    }

    /// `Placemaker.Maker.EndAction` 0x329480: `master.SetDirty()`.
    pub fn end_action(&mut self, host: &mut dyn MakerHost) {
        host.set_dirty();
    }

    /// `Placemaker.Maker.AddClick(hover, voxelType)` 0x328DE0: a voxel at the hover's destination (its vert
    /// refreshed unless full; nothing unless it closes round its corner and the coordinate is allowed): a new
    /// action, the voxel added at once (`Ground` at height 0), `lastAddedVoxel`, `lastActionPos` the
    /// destination's position, the action recorded (`Empty` → type), the world dirty.
    pub fn add_click(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost, hover: &HoverData, voxel_type: VoxelType) -> Option<VoxelId> {
        host.start_counting();
        let height = hover.dst_height;
        let mut vert = hover.dst_vert;
        if !vert.full() {
            vert = grid.get_vert_or_iterate(vert.hex_pos, None);
        }
        if !vert.full() {
            return None;
        }
        if !graph.is_coordinate_allowed(vert.hex_pos, height) {
            return None;
        }
        self.begin_new_action(graph_host, host);
        let t = if height == 0 { VoxelType::Ground } else { voxel_type };
        let h = height as u8;
        let v = graph.add_voxel(grid, graph_host, vert.hex_pos, h, t, true);
        self.last_added_voxel = v;
        self.last_action_pos = hover.get_dst_pos();
        self.add_action(vert.hex_pos, h, VoxelType::Empty, t);
        host.set_dirty();
        v
    }

    /// `Placemaker.Maker.PaintClick(hover, voxelType)` 0x329570: the hovered voxel repainted (nothing when it
    /// has the type already): a new action, the action recorded at the hover's source, the paint, the world
    /// dirty.
    pub fn paint_click(&mut self, graph: &mut Graph, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost, hover: &HoverData, voxel_type: VoxelType) {
        let Some(voxel) = hover.voxel else { return };
        if graph.voxel(voxel).voxel_type == voxel_type {
            return;
        }
        host.start_counting();
        self.begin_new_action(graph_host, host);
        let old = graph.voxel(voxel).voxel_type;
        self.add_action(hover.src_hex_pos(), hover.src_height as u8, old, voxel_type);
        graph.paint_voxel(voxel, voxel_type, None);
        host.set_dirty();
    }

    /// `Placemaker.Maker.RemoveClick(hover)` 0x329800: a new action, `lastActionPos` the hover's source
    /// position, the hovered voxel's action recorded (type → `Empty`) and the voxel removed, the world dirty.
    pub fn remove_click(&mut self, graph: &mut Graph, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost, hover: &HoverData) -> Option<VoxelId> {
        host.start_counting();
        self.begin_new_action(graph_host, host);
        self.last_action_pos = hover.get_src_pos();
        let voxel = hover.voxel.expect("Maker.RemoveClick: NullReferenceException (hover.voxel)");
        let (h, t) = (graph.voxel(voxel).height, graph.voxel(voxel).voxel_type);
        self.add_action(hover.src_hex_pos(), h, t, VoxelType::Empty);
        graph.remove_voxel(graph_host, voxel);
        host.set_dirty();
        Some(voxel)
    }

    /// `Placemaker.Maker.Clear` 0x329270: with any voxels, a new action that removes every voxel, corners
    /// from the last child and each corner's voxels from the top (one action each); the world dirty.
    pub fn clear(&mut self, graph: &mut Graph, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        if !graph.any_voxels() {
            return false;
        }
        self.begin_new_action(graph_host, host);
        let mut i = graph.corners.len() as i32;
        while i > -1 {
            if (i as usize) < graph.corners.len() {
                let c = graph.corners[i as usize];
                let mut k = graph.corner(c).voxels.len() as i32;
                while k > 0 {
                    k -= 1;
                    let v = graph.corner(c).voxels[k as usize];
                    let (h, t) = (graph.voxel(v).height, graph.voxel(v).voxel_type);
                    self.add_action(graph.corner(c).hex_pos, h, t, VoxelType::Empty);
                    graph.remove_voxel(graph_host, v);
                }
            }
            i -= 1;
        }
        host.set_dirty();
        true
    }

    /// `Placemaker.Maker.CanClear` 0x329140: `graph.anyVoxels`.
    pub fn can_clear(&self, graph: &Graph) -> bool {
        graph.any_voxels()
    }

    /// `Placemaker.Maker.CanUndo` 0x3291B0: `actionCount > 0`.
    pub fn can_undo(&self) -> bool {
        self.action_count > 0
    }

    /// `Placemaker.Maker.CanRedo` 0x329170: `actionIndex != actions.Count`.
    pub fn can_redo(&self) -> bool {
        self.action_index != self.actions.len() as i32
    }

    /// `Placemaker.Maker.Undo` 0x329A30: with an action to undo, the hover cleared, the world reset, the
    /// propagation counter stepped; the last action id's actions reversed (`Action.GetReverse`) and applied
    /// with the click effect, newest first; `actionCount--`; the queue-change event.
    pub fn undo(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        if self.action_count == 0 {
            return false;
        }
        host.clear_hover();
        host.reset_state();
        graph_host.wfc_iterate_propagation_count();
        let id = self.actions.get((self.action_index - 1) as usize).expect("Maker.Undo: ArgumentOutOfRangeException").action_id;
        while self.action_index > 0 {
            let a = *self.actions.get((self.action_index - 1) as usize).expect("Maker.Undo: ArgumentOutOfRangeException");
            if a.action_id != id {
                break;
            }
            graph.apply_action(grid, graph_host, a.get_reverse(), true);
            self.action_index -= 1;
        }
        self.action_count -= 1;
        self.action_queue_changed();
        true
    }

    /// `Placemaker.Maker.Redo` 0x329680: with an action to redo, the hover cleared, the world reset, the
    /// propagation counter stepped; the next action id's actions applied with the click effect, oldest first;
    /// `actionCount++`; the queue-change event.
    pub fn redo(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        if self.action_index == self.actions.len() as i32 {
            return false;
        }
        host.clear_hover();
        host.reset_state();
        graph_host.wfc_iterate_propagation_count();
        let id = self.actions.get(self.action_index as usize).expect("Maker.Redo: ArgumentOutOfRangeException").action_id;
        while (self.action_index as usize) < self.actions.len() {
            let a = self.actions[self.action_index as usize];
            if a.action_id != id {
                break;
            }
            graph.apply_action(grid, graph_host, a, true);
            self.action_index += 1;
        }
        self.action_count += 1;
        self.action_queue_changed();
        true
    }

    /// `Placemaker.Maker.UndoCommand` 0x3299F0: the undo sound, then [`undo`](Self::undo).
    pub fn undo_command(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        host.play_ui(MakerClip::Undo);
        self.undo(graph, grid, graph_host, host)
    }

    /// `Placemaker.Maker.RedoCommand` 0x329640: the redo sound, then [`redo`](Self::redo).
    pub fn redo_command(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        host.play_ui(MakerClip::Redo);
        self.redo(graph, grid, graph_host, host)
    }

    /// `Placemaker.Maker.HoldUndo` 0x329510: when the rapid button fires, the undo sound and an undo.
    pub fn hold_undo(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        if !host.rapid_button_press() {
            return false;
        }
        self.undo_command(graph, grid, graph_host, host)
    }

    /// `Placemaker.Maker.HoldRedo` 0x3294B0: when the rapid button fires, the redo sound and a redo.
    pub fn hold_redo(&mut self, graph: &mut Graph, grid: &mut GridGenerator, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) -> bool {
        if !host.rapid_button_press() {
            return false;
        }
        self.redo_command(graph, grid, graph_host, host)
    }

    /// `Placemaker.Maker.ClearCommand` 0x3291C0: [`clear`](Self::clear).
    pub fn clear_command(&mut self, graph: &mut Graph, graph_host: &mut dyn GraphHost, host: &mut dyn MakerHost) {
        self.clear(graph, graph_host, host);
    }
}
