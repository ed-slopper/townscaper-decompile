//! `Placemaker.Maker` (row G1): so far the action list the graph's loading fills (row E4d): `actions`,
//! `actionIndex`, `AddAction`, `ClearUndoQueue`. Clicking, undo and redo come with G1.

use crate::placemaker::graphs::{Action, VoxelType};
use crate::unity::mathematics::Int2;

/// `Placemaker.Maker`: the parts the graph uses.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Maker {
    pub actions: Vec<Action>,
    pub action_index: i32,
    pub action_count: i32,
    pub action_redo_max: i32,
    pub action_id: i32,
    /// The `onQueueChange` events raised, as (actionCount, actionRedoMax - actionCount) (the undo / redo
    /// buttons listen to them).
    pub queue_changes: Vec<(i32, i32)>,
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
}
