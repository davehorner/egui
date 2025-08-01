use std::sync::Arc;

use emath::TSTransform;

use crate::{
    Context, CursorIcon, Event, Galley, Id, LayerId, Pos2, Rect, Response, Ui, layers::ShapeIdx,
    text::CCursor, text_selection::CCursorRange,
};

use super::{
    TextCursorState,
    text_cursor_state::cursor_rect,
    visuals::{RowVertexIndices, paint_text_selection},
};

/// Turn on to help debug this
const DEBUG: bool = false; // Don't merge `true`!

/// One end of a text selection, inside any widget.
#[derive(Clone, Copy)]
struct WidgetTextCursor {
    widget_id: Id,
    ccursor: CCursor,

    /// Last known screen position
    pos: Pos2,
}

impl WidgetTextCursor {
    fn new(
        widget_id: Id,
        cursor: impl Into<CCursor>,
        global_from_galley: TSTransform,
        galley: &Galley,
    ) -> Self {
        let ccursor = cursor.into();
        let pos = global_from_galley * pos_in_galley(galley, ccursor);
        Self {
            widget_id,
            ccursor,
            pos,
        }
    }
}

fn pos_in_galley(galley: &Galley, ccursor: CCursor) -> Pos2 {
    galley.pos_from_cursor(ccursor).center()
}

impl std::fmt::Debug for WidgetTextCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WidgetTextCursor")
            .field("widget_id", &self.widget_id.short_debug_format())
            .field("ccursor", &self.ccursor.index)
            .finish()
    }
}

#[derive(Clone, Copy, Debug)]
struct CurrentSelection {
    /// The selection is in this layer.
    ///
    /// This is to constrain a selection to a single Window.
    pub layer_id: LayerId,

    /// When selecting with a mouse, this is where the mouse was released.
    /// When moving with e.g. shift+arrows, this is what moves.
    /// Note that the two ends can come in any order, and also be equal (no selection).
    pub primary: WidgetTextCursor,

    /// When selecting with a mouse, this is where the mouse was first pressed.
    /// This part of the cursor does not move when shift is down.
    pub secondary: WidgetTextCursor,
}

/// Handles text selection in labels (NOT in [`crate::TextEdit`])s.
///
/// One state for all labels, because we only support text selection in one label at a time.
#[derive(Clone, Debug)]
pub struct LabelSelectionState {
    /// The current selection, if any.
    selection: Option<CurrentSelection>,

    selection_bbox_last_frame: Rect,
    selection_bbox_this_frame: Rect,

    /// Any label hovered this frame?
    any_hovered: bool,

    /// Are we in drag-to-select state?
    is_dragging: bool,

    /// Have we reached the widget containing the primary selection?
    has_reached_primary: bool,

    /// Have we reached the widget containing the secondary selection?
    has_reached_secondary: bool,

    /// Accumulated text to copy.
    text_to_copy: String,
    last_copied_galley_rect: Option<Rect>,

    /// Painted selections this frame.
    ///
    /// Kept so we can undo a bad selection visualization if we don't see both ends of the selection this frame.
    painted_selections: Vec<(ShapeIdx, Vec<RowVertexIndices>)>,
}

impl Default for LabelSelectionState {
    fn default() -> Self {
        Self {
            selection: Default::default(),
            selection_bbox_last_frame: Rect::NOTHING,
            selection_bbox_this_frame: Rect::NOTHING,
            any_hovered: Default::default(),
            is_dragging: Default::default(),
            has_reached_primary: Default::default(),
            has_reached_secondary: Default::default(),
            text_to_copy: Default::default(),
            last_copied_galley_rect: Default::default(),
            painted_selections: Default::default(),
        }
    }
}

impl LabelSelectionState {
    pub(crate) fn register(ctx: &Context) {
        ctx.on_begin_pass("LabelSelectionState", std::sync::Arc::new(Self::begin_pass));
        ctx.on_end_pass("LabelSelectionState", std::sync::Arc::new(Self::end_pass));
    }

    pub fn load(ctx: &Context) -> Self {
        let id = Id::new(ctx.viewport_id());
        ctx.data(|data| data.get_temp::<Self>(id))
            .unwrap_or_default()
    }

    pub fn store(self, ctx: &Context) {
        let id = Id::new(ctx.viewport_id());
        ctx.data_mut(|data| {
            data.insert_temp(id, self);
        });
    }

    fn begin_pass(ctx: &Context) {
        let mut state = Self::load(ctx);

        if ctx.input(|i| i.pointer.any_pressed() && !i.modifiers.shift) {
            // Maybe a new selection is about to begin, but the old one is over:
            // state.selection = None; // TODO(emilk): this makes sense, but doesn't work as expected.
        }

        state.selection_bbox_last_frame = state.selection_bbox_this_frame;
        state.selection_bbox_this_frame = Rect::NOTHING;

        state.any_hovered = false;
        state.has_reached_primary = false;
        state.has_reached_secondary = false;
        state.text_to_copy.clear();
        state.last_copied_galley_rect = None;
        state.painted_selections.clear();

        state.store(ctx);
    }

    fn end_pass(ctx: &Context) {
        let mut state = Self::load(ctx);

        if state.is_dragging {
            ctx.set_cursor_icon(CursorIcon::Text);
        }

        if !state.has_reached_primary || !state.has_reached_secondary {
            // We didn't see both cursors this frame,
            // maybe because they are outside the visible area (scrolling),
            // or one disappeared. In either case we will have horrible glitches, so let's just deselect.

            let prev_selection = state.selection.take();
            if let Some(selection) = prev_selection {
                // This was the first frame of glitch, so hide the
                // glitching by removing all painted selections:
                ctx.graphics_mut(|layers| {
                    if let Some(list) = layers.get_mut(selection.layer_id) {
                        for (shape_idx, row_selections) in state.painted_selections.drain(..) {
                            list.mutate_shape(shape_idx, |shape| {
                                if let epaint::Shape::Text(text_shape) = &mut shape.shape {
                                    let galley = Arc::make_mut(&mut text_shape.galley);
                                    for row_selection in row_selections {
                                        if let Some(placed_row) =
                                            galley.rows.get_mut(row_selection.row)
                                        {
                                            let row = Arc::make_mut(&mut placed_row.row);
                                            for vertex_index in row_selection.vertex_indices {
                                                if let Some(vertex) = row
                                                    .visuals
                                                    .mesh
                                                    .vertices
                                                    .get_mut(vertex_index as usize)
                                                {
                                                    vertex.color = epaint::Color32::TRANSPARENT;
                                                }
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    }
                });
            }
        }

        let pressed_escape = ctx.input(|i| i.key_pressed(crate::Key::Escape));
        let clicked_something_else = ctx.input(|i| i.pointer.any_pressed()) && !state.any_hovered;
        let delected_everything = pressed_escape || clicked_something_else;

        if delected_everything {
            state.selection = None;
        }

        if ctx.input(|i| i.pointer.any_released()) {
            state.is_dragging = false;
        }

        let text_to_copy = std::mem::take(&mut state.text_to_copy);
        if !text_to_copy.is_empty() {
            ctx.copy_text(text_to_copy);
        }

        state.store(ctx);
    }

    pub fn has_selection(&self) -> bool {
        self.selection.is_some()
    }

    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn copy_text(&mut self, new_galley_rect: Rect, galley: &Galley, cursor_range: &CCursorRange) {
        let new_text = selected_text(galley, cursor_range);
        if new_text.is_empty() {
            return;
        }

        if self.text_to_copy.is_empty() {
            self.text_to_copy = new_text;
            self.last_copied_galley_rect = Some(new_galley_rect);
            return;
        }

        let Some(last_copied_galley_rect) = self.last_copied_galley_rect else {
            self.text_to_copy = new_text;
            self.last_copied_galley_rect = Some(new_galley_rect);
            return;
        };

        // We need to append or prepend the new text to the already copied text.
        // We need to do so intelligently.

        if last_copied_galley_rect.bottom() <= new_galley_rect.top() {
            self.text_to_copy.push('\n');
            let vertical_distance = new_galley_rect.top() - last_copied_galley_rect.bottom();
            if estimate_row_height(galley) * 0.5 < vertical_distance {
                self.text_to_copy.push('\n');
            }
        } else {
            let existing_ends_with_space =
                self.text_to_copy.chars().last().map(|c| c.is_whitespace());

            let new_text_starts_with_space_or_punctuation = new_text
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace() || c.is_ascii_punctuation());

            if existing_ends_with_space == Some(false) && !new_text_starts_with_space_or_punctuation
            {
                self.text_to_copy.push(' ');
            }
        }

        self.text_to_copy.push_str(&new_text);
        self.last_copied_galley_rect = Some(new_galley_rect);
    }

    /// Handle text selection state for a label or similar widget.
    ///
    /// Make sure the widget senses clicks and drags.
    ///
    /// This also takes care of painting the galley.
    pub fn label_text_selection(
        ui: &Ui,
        response: &Response,
        galley_pos: Pos2,
        mut galley: Arc<Galley>,
        fallback_color: epaint::Color32,
        underline: epaint::Stroke,
    ) {
        let mut state = Self::load(ui.ctx());
        let new_vertex_indices = state.on_label(ui, response, galley_pos, &mut galley);

        let shape_idx = ui.painter().add(
            epaint::TextShape::new(galley_pos, galley, fallback_color).with_underline(underline),
        );

        if !new_vertex_indices.is_empty() {
            state
                .painted_selections
                .push((shape_idx, new_vertex_indices));
        }

        state.store(ui.ctx());
    }

    fn cursor_for(
        &mut self,
        ui: &Ui,
        response: &Response,
        global_from_galley: TSTransform,
        galley: &Galley,
    ) -> TextCursorState {
        let Some(selection) = &mut self.selection else {
            // Nothing selected.
            return TextCursorState::default();
        };

        if selection.layer_id != response.layer_id {
            // Selection is in another layer
            return TextCursorState::default();
        }

        let galley_from_global = global_from_galley.inverse();

        let multi_widget_text_select = ui.style().interaction.multi_widget_text_select;

        let may_select_widget =
            multi_widget_text_select || selection.primary.widget_id == response.id;

        if self.is_dragging && may_select_widget {
            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                let galley_rect =
                    global_from_galley * Rect::from_min_size(Pos2::ZERO, galley.size());
                let galley_rect = galley_rect.intersect(ui.clip_rect());

                let is_in_same_column = galley_rect
                    .x_range()
                    .intersects(self.selection_bbox_last_frame.x_range());

                let has_reached_primary =
                    self.has_reached_primary || response.id == selection.primary.widget_id;
                let has_reached_secondary =
                    self.has_reached_secondary || response.id == selection.secondary.widget_id;

                let new_primary = if response.contains_pointer() {
                    // Dragging into this widget - easy case:
                    Some(galley.cursor_from_pos((galley_from_global * pointer_pos).to_vec2()))
                } else if is_in_same_column
                    && !self.has_reached_primary
                    && selection.primary.pos.y <= selection.secondary.pos.y
                    && pointer_pos.y <= galley_rect.top()
                    && galley_rect.top() <= selection.secondary.pos.y
                {
                    // The user is dragging the text selection upwards, above the first selected widget (this one):
                    if DEBUG {
                        ui.ctx()
                            .debug_text(format!("Upwards drag; include {:?}", response.id));
                    }
                    Some(galley.begin())
                } else if is_in_same_column
                    && has_reached_secondary
                    && has_reached_primary
                    && selection.secondary.pos.y <= selection.primary.pos.y
                    && selection.secondary.pos.y <= galley_rect.bottom()
                    && galley_rect.bottom() <= pointer_pos.y
                {
                    // The user is dragging the text selection downwards, below this widget.
                    // We move the cursor to the end of this widget,
                    // (and we may do the same for the next widget too).
                    if DEBUG {
                        ui.ctx()
                            .debug_text(format!("Downwards drag; include {:?}", response.id));
                    }
                    Some(galley.end())
                } else {
                    None
                };

                if let Some(new_primary) = new_primary {
                    selection.primary =
                        WidgetTextCursor::new(response.id, new_primary, global_from_galley, galley);

                    // We don't want the latency of `drag_started`.
                    let drag_started = ui.input(|i| i.pointer.any_pressed());
                    if drag_started {
                        if selection.layer_id == response.layer_id {
                            if ui.input(|i| i.modifiers.shift) {
                                // A continuation of a previous selection.
                            } else {
                                // A new selection in the same layer.
                                selection.secondary = selection.primary;
                            }
                        } else {
                            // A new selection in a new layer.
                            selection.layer_id = response.layer_id;
                            selection.secondary = selection.primary;
                        }
                    }
                }
            }
        }

        let has_primary = response.id == selection.primary.widget_id;
        let has_secondary = response.id == selection.secondary.widget_id;

        if has_primary {
            selection.primary.pos =
                global_from_galley * pos_in_galley(galley, selection.primary.ccursor);
        }
        if has_secondary {
            selection.secondary.pos =
                global_from_galley * pos_in_galley(galley, selection.secondary.ccursor);
        }

        self.has_reached_primary |= has_primary;
        self.has_reached_secondary |= has_secondary;

        let primary = has_primary.then_some(selection.primary.ccursor);
        let secondary = has_secondary.then_some(selection.secondary.ccursor);

        // The following code assumes we will encounter both ends of the cursor
        // at some point (but in any order).
        // If we don't (e.g. because one endpoint is outside the visible scroll areas),
        // we will have annoying failure cases.

        match (primary, secondary) {
            (Some(primary), Some(secondary)) => {
                // This is the only selected label.
                TextCursorState::from(CCursorRange {
                    primary,
                    secondary,
                    h_pos: None,
                })
            }

            (Some(primary), None) => {
                // This labels contains only the primary cursor.
                let secondary = if self.has_reached_secondary {
                    // Secondary was before primary.
                    // Select everything up to the cursor.
                    // We assume normal left-to-right and top-down layout order here.
                    galley.begin()
                } else {
                    // Select everything from the cursor onward:
                    galley.end()
                };
                TextCursorState::from(CCursorRange {
                    primary,
                    secondary,
                    h_pos: None,
                })
            }

            (None, Some(secondary)) => {
                // This labels contains only the secondary cursor
                let primary = if self.has_reached_primary {
                    // Primary was before secondary.
                    // Select everything up to the cursor.
                    // We assume normal left-to-right and top-down layout order here.
                    galley.begin()
                } else {
                    // Select everything from the cursor onward:
                    galley.end()
                };
                TextCursorState::from(CCursorRange {
                    primary,
                    secondary,
                    h_pos: None,
                })
            }

            (None, None) => {
                // This widget has neither the primary or secondary cursor.
                let is_in_middle = self.has_reached_primary != self.has_reached_secondary;
                if is_in_middle {
                    if DEBUG {
                        response.ctx.debug_text(format!(
                            "widget in middle: {:?}, between {:?} and {:?}",
                            response.id, selection.primary.widget_id, selection.secondary.widget_id,
                        ));
                    }
                    // …but it is between the two selection endpoints, and so is fully selected.
                    TextCursorState::from(CCursorRange::two(galley.begin(), galley.end()))
                } else {
                    // Outside the selected range
                    TextCursorState::default()
                }
            }
        }
    }

    /// Returns the painted selections, if any.
    fn on_label(
        &mut self,
        ui: &Ui,
        response: &Response,
        galley_pos_in_layer: Pos2,
        galley: &mut Arc<Galley>,
    ) -> Vec<RowVertexIndices> {
        let widget_id = response.id;

        let global_from_layer = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or_default();
        let layer_from_galley = TSTransform::from_translation(galley_pos_in_layer.to_vec2());
        let galley_from_layer = layer_from_galley.inverse();
        let layer_from_global = global_from_layer.inverse();
        let galley_from_global = galley_from_layer * layer_from_global;
        let global_from_galley = global_from_layer * layer_from_galley;

        if response.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Text);
        }

        self.any_hovered |= response.hovered();
        self.is_dragging |= response.is_pointer_button_down_on(); // we don't want the initial latency of drag vs click decision

        let old_selection = self.selection;

        let mut cursor_state = self.cursor_for(ui, response, global_from_galley, galley);

        let old_range = cursor_state.range(galley);

        if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
            if response.contains_pointer() {
                let cursor_at_pointer =
                    galley.cursor_from_pos((galley_from_global * pointer_pos).to_vec2());

                // This is where we handle start-of-drag and double-click-to-select.
                // Actual drag-to-select happens elsewhere.
                let dragged = false;
                cursor_state.pointer_interaction(ui, response, cursor_at_pointer, galley, dragged);
            }
        }

        if let Some(mut cursor_range) = cursor_state.range(galley) {
            let galley_rect = global_from_galley * Rect::from_min_size(Pos2::ZERO, galley.size());
            self.selection_bbox_this_frame |= galley_rect;

            if let Some(selection) = &self.selection {
                if selection.primary.widget_id == response.id {
                    process_selection_key_events(ui.ctx(), galley, response.id, &mut cursor_range);
                }
            }

            if got_copy_event(ui.ctx()) {
                self.copy_text(galley_rect, galley, &cursor_range);
            }

            cursor_state.set_char_range(Some(cursor_range));
        }

        // Look for changes due to keyboard and/or mouse interaction:
        let new_range = cursor_state.range(galley);
        let selection_changed = old_range != new_range;

        if let (true, Some(range)) = (selection_changed, new_range) {
            // --------------
            // Store results:

            if let Some(selection) = &mut self.selection {
                let primary_changed = Some(range.primary) != old_range.map(|r| r.primary);
                let secondary_changed = Some(range.secondary) != old_range.map(|r| r.secondary);

                selection.layer_id = response.layer_id;

                if primary_changed || !ui.style().interaction.multi_widget_text_select {
                    selection.primary =
                        WidgetTextCursor::new(widget_id, range.primary, global_from_galley, galley);
                    self.has_reached_primary = true;
                }
                if secondary_changed || !ui.style().interaction.multi_widget_text_select {
                    selection.secondary = WidgetTextCursor::new(
                        widget_id,
                        range.secondary,
                        global_from_galley,
                        galley,
                    );
                    self.has_reached_secondary = true;
                }
            } else {
                // Start of a new selection
                self.selection = Some(CurrentSelection {
                    layer_id: response.layer_id,
                    primary: WidgetTextCursor::new(
                        widget_id,
                        range.primary,
                        global_from_galley,
                        galley,
                    ),
                    secondary: WidgetTextCursor::new(
                        widget_id,
                        range.secondary,
                        global_from_galley,
                        galley,
                    ),
                });
                self.has_reached_primary = true;
                self.has_reached_secondary = true;
            }
        }

        // Scroll containing ScrollArea on cursor change:
        if let Some(range) = new_range {
            let old_primary = old_selection.map(|s| s.primary);
            let new_primary = self.selection.as_ref().map(|s| s.primary);
            if let Some(new_primary) = new_primary {
                let primary_changed = old_primary.is_none_or(|old| {
                    old.widget_id != new_primary.widget_id || old.ccursor != new_primary.ccursor
                });
                if primary_changed && new_primary.widget_id == widget_id {
                    let is_fully_visible = ui.clip_rect().contains_rect(response.rect); // TODO(emilk): remove this HACK workaround for https://github.com/emilk/egui/issues/1531
                    if selection_changed && !is_fully_visible {
                        // Scroll to keep primary cursor in view:
                        let row_height = estimate_row_height(galley);
                        let primary_cursor_rect =
                            global_from_galley * cursor_rect(galley, &range.primary, row_height);
                        ui.scroll_to_rect(primary_cursor_rect, None);
                    }
                }
            }
        }

        let cursor_range = cursor_state.range(galley);

        let mut new_vertex_indices = vec![];

        if let Some(cursor_range) = cursor_range {
            paint_text_selection(
                galley,
                ui.visuals(),
                &cursor_range,
                Some(&mut new_vertex_indices),
            );
        }

        #[cfg(feature = "accesskit")]
        super::accesskit_text::update_accesskit_for_text_widget(
            ui.ctx(),
            response.id,
            cursor_range,
            accesskit::Role::Label,
            global_from_galley,
            galley,
        );

        new_vertex_indices
    }
}

fn got_copy_event(ctx: &Context) -> bool {
    ctx.input(|i| {
        i.events
            .iter()
            .any(|e| matches!(e, Event::Copy | Event::Cut))
    })
}

/// Returns true if the cursor changed
fn process_selection_key_events(
    ctx: &Context,
    galley: &Galley,
    widget_id: Id,
    cursor_range: &mut CCursorRange,
) -> bool {
    let os = ctx.os();

    let mut changed = false;

    ctx.input(|i| {
        // NOTE: we have a lock on ui/ctx here,
        // so be careful to not call into `ui` or `ctx` again.
        for event in &i.events {
            changed |= cursor_range.on_event(os, event, galley, widget_id);
        }
    });

    changed
}

fn selected_text(galley: &Galley, cursor_range: &CCursorRange) -> String {
    // This logic means we can select everything in an elided label (including the `…`)
    // and still copy the entire un-elided text!
    let everything_is_selected = cursor_range.contains(CCursorRange::select_all(galley));

    let copy_everything = cursor_range.is_empty() || everything_is_selected;

    if copy_everything {
        galley.text().to_owned()
    } else {
        cursor_range.slice_str(galley).to_owned()
    }
}

fn estimate_row_height(galley: &Galley) -> f32 {
    if let Some(placed_row) = galley.rows.first() {
        placed_row.height()
    } else {
        galley.size().y
    }
}
