// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

//! The pluguzu UI.

use egui_code_editor::{CodeEditor, ColorTheme};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor, egui,
    egui::{FontId, RichText, TextStyle, Vec2},
    resizable_window::ResizableWindow,
};

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use egui::{Color32, Shape, Stroke, StrokeKind};

use crate::events::LocEvent;
use crate::syntax::tidal;
use crate::{highlights, presets};
use crate::{ParseTask, Pluguzu, PluguzuTask};

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

struct UIState {
    view: UIView,
    preset: PresetState,
    highlights: BTreeMap<LocEvent, f64>,
    focused: bool,
    updated: bool,
    unsaved: bool,
    update: Option<ParseTask>,
    picked_path: Option<String>,
    hosted_in_daw: bool,
}

const PLAY_TIP: &str = "shift-enter triggers direct play and alt-enter queues to the next bar…";

impl Pluguzu {
    pub fn do_editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let egui_state = params.editor_state.clone();
        let code = params.code.clone();
        let font_size = params.font_size.clone();
        let initial_font_size = font_size.load(Ordering::Relaxed);
        let initial_file = self.initial_file.clone();
        let initial_hosted_in_daw = self.hosted_in_daw.clone();

        let dsp_state = self.dsp_state.clone();

        let evlocs = self.evlocs.clone();
        let evlocs_pos = self.evlocs_pos.clone();

        let state = UIState {
            view: UIView::Editor,
            preset: PresetState {
                cat: 0,
                selected: 0,
                presets: load_presets(),
            },
            highlights: BTreeMap::new(),
            focused: false,
            updated: false,
            update: None,
            unsaved: false,
            picked_path: None,
            hosted_in_daw: false,
        };

        create_egui_editor(
            self.params.editor_state.clone(),
            state,
            move |egui_ctx, state| {
                state.hosted_in_daw = initial_hosted_in_daw.load(Ordering::Relaxed);
                state.picked_path = initial_file.lock().unwrap().take();
                update_font_size(egui_ctx, initial_font_size);
            },
            move |egui_ctx, _setter, state| {
                // TODO: handle input dropped_files!
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(80.0, 60.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        // Main UI
                        let fs = font_size.load(Ordering::Relaxed);
                        match state.view {
                            UIView::Preset => {
                                if let Some(changed) = preset_ui(ui, &mut state.preset, fs) {
                                    if let Some(new_code) = changed {
                                        *code.lock().expect("lock") = new_code;
                                        async_executor.execute_background(PluguzuTask::Parse(
                                            ParseTask::InPlace,
                                        ));
                                    }
                                    state.view = UIView::Editor;
                                };
                            }
                            UIView::Editor => {
                                let mut font_size_value = fs;
                                let dsp_state = dsp_state.read().expect("lock");
                                egui::MenuBar::new().ui(ui, |ui| {
                                    let style = ui.style_mut();
                                    style.spacing.item_spacing.x = 8.;
                                    if ui.button("+").clicked() {
                                        font_size_value = font_size_value.saturating_add(1);
                                        font_size.store(font_size_value, Ordering::Relaxed);
                                        update_font_size(egui_ctx, font_size_value);
                                    }
                                    if font_size_value > 4 && ui.button("-").clicked() {
                                        font_size_value = font_size_value.saturating_sub(1);
                                        font_size.store(font_size_value, Ordering::Relaxed);
                                        update_font_size(egui_ctx, font_size_value);
                                    }
                                    let file_menu = if state.unsaved
                                        && (!state.hosted_in_daw || state.picked_path.is_some())
                                    {
                                        "File*"
                                    } else {
                                        "File"
                                    };
                                    ui.menu_button(file_menu, |ui| {
                                        if ui.button("New").clicked() {
                                            *code.lock().expect("lock") = "".to_string();
                                            async_executor.execute_background(PluguzuTask::Parse(
                                                ParseTask::InPlace,
                                            ));
                                        }
                                        if ui.button("Open").clicked()
                                            && let Some(path) = file_dialog(false)
                                        {
                                            state.unsaved = false;
                                            state.picked_path = Some(path.display().to_string());
                                            async_executor
                                                .execute_background(PluguzuTask::LoadCode(path))
                                        }
                                        if state.unsaved
                                            && let Some(path) = &state.picked_path
                                            && ui.button("Save").on_hover_text("ctrl-s").clicked()
                                        {
                                            state.unsaved = false;
                                            async_executor.execute_background(
                                                PluguzuTask::SaveCode(path.into()),
                                            )
                                        }

                                        if state.unsaved
                                            && ui.button("Save-As").clicked()
                                            && let Some(path) = file_dialog(true)
                                        {
                                            state.unsaved = false;
                                            state.picked_path = Some(path.display().to_string());
                                            async_executor
                                                .execute_background(PluguzuTask::SaveCode(path))
                                        }
                                        if ui.button("Load Preset").clicked() {
                                            state.view = UIView::Preset
                                        }
                                    });
                                    let status = if dsp_state.playing { ">>=" } else { "|-|" };
                                    let next = dsp_state.next;
                                    let remaining = dsp_state.remaining;
                                    ui.label(RichText::new(format!(
                                        "{status} {next} | {remaining} events"
                                    )));
                                    if state.updated
                                        && ui.button("Play").on_hover_text(PLAY_TIP).clicked()
                                    {
                                        state.update = Some(ParseTask::NoRender);
                                    }
                                    if let Some(path) = state.picked_path.as_ref() {
                                        ui.label(file_name(path));
                                    }
                                });

                                let mut theme = ColorTheme::SONOKAI;
                                let status_line = if dsp_state.error.is_some() {
                                    theme.cursor = "#ff0000";
                                    1
                                } else {
                                    0
                                };

                                let line_height = font_size_value as f32 + VERT_SPACE;
                                let avail_lines = (ui.available_height() / line_height).floor()
                                    - status_line as f32;

                                let mut code_editor = CodeEditor::default()
                                    .id_source("code editor")
                                    .with_rows(avail_lines as usize)
                                    .with_fontsize(font_size_value as f32)
                                    .with_theme(theme)
                                    .with_syntax(tidal())
                                    .with_numlines(true);

                                let mut code_data = code.lock().expect("lock");
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
                                        && let Some(path) = &state.picked_path
                                    {
                                        state.unsaved = false;
                                        async_executor
                                            .execute_background(PluguzuTask::SaveCode(path.into()));
                                        None
                                    } else {
                                        None
                                    }
                                });
                                let output = if let Some(task) = enter_update {
                                    state.update = Some(task);

                                    // Render the code editor with read-only code to avoid inserting the new-line.
                                    let mut code_data: &str = code_data.deref();
                                    code_editor.show(ui, &mut code_data)
                                } else {
                                    code_editor.show(ui, code_data.deref_mut())
                                };

                                if !state.focused {
                                    output.response.request_focus();
                                    state.focused = true;
                                }

                                let painter = ui.painter_at(output.response.rect);
                                let ep = highlights::EditorPixels::new(
                                    output.galley_pos,
                                    &output.galley,
                                );

                                // Remove old highlights
                                state
                                    .highlights
                                    .retain(|_, when| dsp_state.pos - *when < 0.5);

                                // Add new highlights
                                let evlocs = evlocs.read().expect("lock");
                                if dsp_state.playing {
                                    let mut pos = evlocs_pos.load(Ordering::Relaxed) as usize;
                                    while pos < evlocs.len() {
                                        let event = &evlocs[pos];
                                        let start = event.span.start;
                                        if dsp_state.pos < start {
                                            break;
                                        }

                                        for loc in &event.locs {
                                            // This doesn't replace existing highlights to avoid fast blinkin
                                            state.highlights.entry(*loc).or_insert(start);
                                        }
                                        pos += 1;
                                    }
                                    evlocs_pos.store(pos as u32, Ordering::Relaxed);
                                }

                                let shapes = state.highlights.iter().map(|(el, when)| {
                                    let alpha = (egui::lerp(
                                        1.0..=0.3,
                                        (dsp_state.pos - when) / 0.5,
                                    ) * 255.) as u8;
                                    Shape::rect_stroke(
                                        ep.highlight_rect(el.col, el.row, el.len),
                                        5.,
                                        Stroke {
                                            width: 2.,
                                            color: Color32::from_rgba_unmultiplied(
                                                0xff, 0x6e, 0xc7, alpha,
                                            ),
                                        },
                                        StrokeKind::Middle,
                                    )
                                });
                                painter.extend(shapes);

                                if status_line > 0 {
                                    ui.horizontal(|ui| {
                                        if let Some(error) = dsp_state.error.as_deref() {
                                            ui.label("Error: ");
                                            ui.label(error);
                                        }
                                    });
                                }

                                if let Some(task) = state.update.take() {
                                    state.updated = false;
                                    async_executor.execute_background(PluguzuTask::Parse(task));
                                    output.response.request_focus();
                                } else if output.response.changed() {
                                    state.updated = true;
                                    state.unsaved = true;
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

fn update_font_size(ctx: &egui::Context, font_size: u8) {
    ctx.all_styles_mut(|style| {
        // println!("All style: {:?}", style.text_styles);
        let font = FontId::monospace(font_size as f32);
        style.text_styles.insert(TextStyle::Button, font.clone());
        style.text_styles.insert(TextStyle::Body, font);
        // Make the UI more tight
        style.spacing.item_spacing.x = 4.;
    });
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

fn preset_ui(ui: &mut egui::Ui, state: &mut PresetState, font_size: u8) -> Option<Option<String>> {
    // Tag selector
    let prev_cat = state.cat;
    let cat = &mut state.cat;
    ui.horizontal(|ui| {
        for (pos, (tag, _)) in state.presets.iter().enumerate() {
            ui.selectable_value(cat, pos, tag);
        }
    });
    if prev_cat != *cat {
        state.selected = 0;
    }

    ui.separator();
    // Preset selector
    let sel = &mut state.selected;
    let mut result = None;
    if let Some((_, presets)) = state.presets.get(*cat) {
        let line_height = font_size as f32 + VERT_SPACE;
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
