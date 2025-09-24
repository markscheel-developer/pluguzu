// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The pluguzu UI.

use egui_code_editor::{CodeEditor, ColorTheme};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor, egui,
    egui::{text_edit::TextEditOutput, FontId, RichText, TextStyle, Vec2},
    resizable_window::ResizableWindow,
};

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use egui::{Color32, Shape, Stroke, StrokeKind};

use crate::events::LocEvent;
use crate::syntax::tidal;
use crate::{highlights, presets};
use crate::{BackgroundState, ParseTask, Pluguzu, PluguzuTask};

const VERT_SPACE: f32 = 3.;

enum UIView {
    Editor,
    Preset,
}

struct PresetState {
    cat: usize,
    selected: usize,
    presets: Presets,
}

struct EditorState {
    code: Arc<Mutex<String>>,
    font_size: f32,

    highlights: BTreeMap<LocEvent, f64>,

    focused: bool,
    updated: bool,
    unsaved: bool,

    update: Option<PluguzuTask>,
    picked_path: Option<String>,
}

struct UIState {
    view: UIView,
    editor: EditorState,
    preset: PresetState,
    font_size: Arc<AtomicU8>,
    hosted_in_daw: bool,
    background_state: Arc<BackgroundState>,
}

const PLAY_TIP: &str = "shift-enter triggers direct play and alt-enter queues to the next bar…";

impl Pluguzu {
    pub fn do_editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let egui_state = self.params.editor_state.clone();
        // These two initial values are not available yet, they will be used in the create_egui_editor init call back
        let initial_file = self.initial_file.clone();
        let initial_hosted_in_daw = self.hosted_in_daw.clone();

        let dsp_state = self.dsp_state.clone();

        let state = UIState {
            view: UIView::Editor,
            preset: PresetState {
                cat: 0,
                selected: 0,
                presets: load_presets(),
            },
            editor: EditorState {
                code: self.params.code.clone(),
                font_size: 0.,
                highlights: BTreeMap::new(),
                focused: false,
                updated: false,
                unsaved: false,
                update: None,
                picked_path: None,
            },
            font_size: self.params.font_size.clone(),
            background_state: self.background_state.clone(),
            hosted_in_daw: false,
        };
        let params = self.params.clone();
        create_egui_editor(
            self.params.editor_state.clone(),
            state,
            move |egui_ctx, state| {
                state.hosted_in_daw = initial_hosted_in_daw.load(Ordering::Relaxed);
                state.editor.picked_path = initial_file.lock().unwrap().take();
                state.update_font_size(egui_ctx, state.font_size.load(Ordering::Relaxed));
            },
            move |egui_ctx, setter, state| {
                // TODO: handle input dropped_files!
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(80.0, 60.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        // Main UI
                        match state.view {
                            UIView::Preset => {
                                if let Some(changed) = state.preset.show(ui, state.editor.font_size)
                                {
                                    if let Some(new_code) = changed {
                                        state.editor.set_code(new_code);
                                        async_executor.execute_background(PluguzuTask::Parse(
                                            ParseTask::InPlace,
                                        ));
                                    }
                                    state.view = UIView::Editor;
                                };
                            }
                            UIView::Editor => {
                                // Top-bar
                                egui::MenuBar::new().ui(ui, |ui| {
                                    let style = ui.style_mut();
                                    style.spacing.item_spacing.x = 8.;
                                    if ui.button("+").clicked() {
                                        state.adjust_font_size(egui_ctx, 1);
                                    }
                                    if state.editor.font_size > 4. && ui.button("-").clicked() {
                                        state.adjust_font_size(egui_ctx, -1);
                                    }

                                    let dirty = state.editor.unsaved
                                        && (!state.hosted_in_daw // not in a DAW, or editing a file...
                                            || state.editor.picked_path.is_some());
                                    let file_menu = if dirty { "File*" } else { "File" };
                                    ui.menu_button(file_menu, |ui| {
                                        state.editor.file_menu(ui, &mut state.view)
                                    });
                                    state.editor.render_status(ui, &dsp_state);
                                });

                                // Code editor
                                let error = state.background_state.error.lock().expect("lock");
                                let status_line = if error.is_some() { 1 } else { 0 };
                                let output = state.editor.code_editor(ui, status_line);
                                state.editor.render_highlights(
                                    ui,
                                    &dsp_state,
                                    &state.background_state,
                                    &output,
                                );

                                // Bottom-bar
                                if status_line > 0 {
                                    ui.horizontal(|ui| {
                                        if let Some(error) = error.as_deref() {
                                            ui.label("Error: ");
                                            ui.label(error);
                                        }
                                    });
                                }

                                if let Some(task) = state.editor.update.take() {
                                    if let PluguzuTask::Parse(_) = task {
                                        if state.editor.updated {
                                            // Tell the host that the project needs to be saved
                                            setter.begin_set_parameter(&params.version);
                                            setter.set_parameter(
                                                &params.version,
                                                params.version.value().overflowing_add(1).0,
                                            );
                                            setter.end_set_parameter(&params.version);
                                        }
                                        state.editor.updated = false;
                                    }
                                    async_executor.execute_background(task);
                                    output.response.request_focus();
                                } else if output.response.changed() {
                                    state.editor.updated = true;
                                    state.editor.unsaved = true;
                                    // TODO: sometime alt modifier get stuck after clicking, perhaps it could be manually removed to workaround this weird bug?
                                    // egui_ctx.input(|i| println!("HERE: {:?}", i.modifiers));
                                    // async_executor.execute_background(PluguzuTask::Parse(true))
                                }
                            }
                        };
                    });
            },
        )
    }
}

impl UIState {
    fn adjust_font_size(&mut self, ctx: &egui::Context, val: i8) {
        let mut font_size = self.font_size.load(Ordering::Relaxed);
        if val > 0 {
            font_size = font_size.saturating_add(1);
        } else {
            font_size = font_size.saturating_sub(1);
        }
        self.update_font_size(ctx, font_size);
    }
    fn update_font_size(&mut self, ctx: &egui::Context, font_size: u8) {
        self.editor.font_size = font_size as f32;
        self.font_size.store(font_size, Ordering::Relaxed);
        ctx.all_styles_mut(|style| {
            let font = FontId::monospace(font_size as f32);
            style.text_styles.insert(TextStyle::Button, font.clone());
            style.text_styles.insert(TextStyle::Body, font);
            // Make the UI more tight
            style.spacing.item_spacing.x = 4.;
        });
    }
}

type Presets = Vec<(String, Vec<Arc<presets::UzuProgram>>)>;

fn load_presets() -> Presets {
    let presets = presets::presets();
    let mut map = indexmap::IndexMap::with_capacity(presets.len());
    for preset in presets {
        let preset = Arc::new(preset);
        for tag in &preset.tags {
            map.entry(tag.to_string())
                .or_insert_with(Vec::new)
                .push(preset.clone())
        }
    }
    map.into_iter().collect()
}

impl PresetState {
    fn show(&mut self, ui: &mut egui::Ui, font_size: f32) -> Option<Option<String>> {
        // Tag selector
        let prev_cat = self.cat;
        let cat = &mut self.cat;
        ui.horizontal(|ui| {
            for (pos, (tag, _)) in self.presets.iter().enumerate() {
                ui.selectable_value(cat, pos, tag);
            }
        });
        if prev_cat != *cat {
            self.selected = 0;
        }

        ui.separator();
        // Preset selector
        let sel = &mut self.selected;
        let mut result = None;
        if let Some((_, presets)) = self.presets.get(*cat) {
            let line_height = font_size + VERT_SPACE;
            let avail_height = ui.available_height() - line_height * 2.0 + 10.;
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .max_height(avail_height)
                .show(ui, |ui| {
                    egui::Grid::new("my_grid")
                        .num_columns(2)
                        .spacing([6.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            for (pos, preset) in presets.iter().enumerate() {
                                ui.selectable_value(sel, pos, preset.name);
                                ui.label(format!("{}", preset.code.len()));
                                ui.end_row();
                            }
                        });
                    if let Some(preset) = presets.get(*sel)
                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                    {
                        result = Some(Some(preset.code.to_string()));
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        result = Some(None);
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                        *sel = sel.saturating_sub(1)
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                        *sel = sel.saturating_add(1)
                    }
                });
            ui.horizontal(|ui| {
                if let Some(preset) = presets.get(*sel)
                    && ui.button("Load").clicked()
                {
                    result = Some(Some(preset.code.to_string()))
                };
                if ui.button("Cancel").clicked() {
                    result = Some(None)
                };
            });
        } else if ui.button("Oops").clicked() {
            result = Some(None)
        }
        result
    }
}

impl EditorState {
    fn set_code(&self, code: String) {
        *self.code.lock().expect("lock") = code;
    }
    fn code_editor(&mut self, ui: &mut egui::Ui, status_line: i16) -> TextEditOutput {
        let mut theme = ColorTheme::SONOKAI;
        if status_line > 0 {
            theme.cursor = "#ff0000";
        }
        let line_height = self.font_size + VERT_SPACE;
        let avail_lines = (ui.available_height() / line_height).floor() - status_line as f32;

        let mut code_editor = CodeEditor::default()
            .id_source("code editor")
            .with_rows(avail_lines as usize)
            .with_fontsize(self.font_size)
            .with_theme(theme)
            .with_syntax(tidal())
            .with_numlines(true);

        let enter_update = ui.input(|i| {
            if i.key_pressed(egui::Key::Enter) {
                if i.modifiers.shift {
                    Some(ParseTask::InPlace)
                } else if i.modifiers.alt {
                    Some(ParseTask::Next)
                } else {
                    None
                }
            } else if i.key_pressed(egui::Key::S)
                && i.modifiers.ctrl
                && let Some(path) = &self.picked_path
            {
                self.unsaved = false;
                self.update = Some(PluguzuTask::SaveCode(path.into()));
                None
            } else {
                None
            }
        });
        let mut code_data = self.code.lock().expect("lock");
        let output = if let Some(task) = enter_update {
            self.update = Some(PluguzuTask::Parse(task));

            // Render the code editor with read-only code to avoid inserting the new-line.
            let mut code_data: &str = code_data.deref();
            code_editor.show(ui, &mut code_data)
        } else {
            code_editor.show(ui, code_data.deref_mut())
        };
        if !self.focused {
            output.response.request_focus();
            self.focused = true;
        }
        output
    }
    fn file_menu(&mut self, ui: &mut egui::Ui, view: &mut UIView) {
        if ui.button("New").clicked() {
            self.set_code("".to_string());
            self.picked_path = None;
            self.update = Some(PluguzuTask::Parse(ParseTask::InPlace));
        }

        if ui.button("Open").clicked()
            && let Some(path) = file_dialog(false)
        {
            self.unsaved = false;
            self.picked_path = Some(path.display().to_string());
            self.update = Some(PluguzuTask::LoadCode(path))
        }
        if self.unsaved
            && let Some(path) = &self.picked_path
            && ui.button("Save").on_hover_text("ctrl-s").clicked()
        {
            self.unsaved = false;
            self.update = Some(PluguzuTask::SaveCode(path.into()));
        }

        if self.unsaved
            && ui.button("Save-As").clicked()
            && let Some(path) = file_dialog(true)
        {
            self.unsaved = false;
            self.picked_path = Some(path.display().to_string());
            self.update = Some(PluguzuTask::SaveCode(path))
        }
        if ui.button("Load Preset").clicked() {
            *view = UIView::Preset
        }
    }
    fn render_status(&mut self, ui: &mut egui::Ui, dsp_state: &crate::PluguzuDSP) {
        let playing = dsp_state.playing.load(Ordering::Relaxed);
        let status = if playing { ">>=" } else { "|-|" };
        let next = dsp_state.next.load(Ordering::Relaxed);
        let remaining = dsp_state.remaining.load(Ordering::Relaxed);
        ui.label(RichText::new(format!(
            "{status} {next} | {remaining} events"
        )));
        if self.updated && ui.button("Play").on_hover_text(PLAY_TIP).clicked() {
            self.update = Some(PluguzuTask::Parse(ParseTask::InPlace));
        }
        if let Some(path) = self.picked_path.as_ref() {
            ui.label("  | ");
            ui.label(file_name(path));
        }
    }
    fn render_highlights(
        &mut self,
        ui: &mut egui::Ui,
        dsp_state: &crate::PluguzuDSP,
        background_state: &BackgroundState,
        output: &TextEditOutput,
    ) {
        let painter = ui.painter_at(output.response.rect);
        let ep = highlights::EditorPixels::new(output.galley_pos, &output.galley);
        let dsp_state_pos = dsp_state.pos.load(Ordering::Relaxed);

        // Remove old highlights
        self.highlights
            .retain(|_, when| dsp_state_pos - *when < 0.5);

        // Add new highlights
        if dsp_state.playing.load(Ordering::Relaxed) {
            let mut evlocs_pos = background_state.evlocs_pos.load(Ordering::Relaxed);
            let initial_pos = evlocs_pos;
            let evlocs = background_state.evlocs.load();
            while evlocs_pos < evlocs.len() {
                let event = &evlocs[evlocs_pos];
                let start = event.span.start;
                if dsp_state_pos < start {
                    break;
                }

                for loc in &event.locs {
                    // This doesn't replace existing highlights to avoid fast blinkin
                    self.highlights.entry(*loc).or_insert(start);
                }
                evlocs_pos += 1;
            }
            let _ = background_state.evlocs_pos.fetch_update(
                Ordering::Relaxed,
                Ordering::Relaxed,
                |old_pos| {
                    // Only update the pos if the main thread didn't update it in the meantime.
                    if initial_pos == old_pos {
                        Some(evlocs_pos)
                    } else {
                        None
                    }
                },
            );
        }

        let shapes = self.highlights.iter().map(|(el, when)| {
            let alpha = (egui::lerp(1.0..=0.3, (dsp_state_pos - when) / 0.5) * 255.) as u8;
            Shape::rect_stroke(
                ep.highlight_rect(el.col, el.row, el.len),
                5.,
                Stroke {
                    width: 2.,
                    color: Color32::from_rgba_unmultiplied(0xff, 0x6e, 0xc7, alpha),
                },
                StrokeKind::Middle,
            )
        });
        painter.extend(shapes);
    }
}

fn file_name(s: &str) -> &str {
    if let Some(p) = s.rfind('/') {
        &s[p + 1..]
    } else {
        s
    }
}

fn file_dialog(save: bool) -> Option<std::path::PathBuf> {
    let diag = rfd::FileDialog::new().add_filter("code", &["hs"]);
    if save {
        diag.save_file()
    } else {
        diag.pick_file()
    }
}
