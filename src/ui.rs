// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use egui_code_editor::{CodeEditor, ColorTheme};
use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor, egui,
    egui::{FontId, RichText, TextStyle, Vec2},
    resizable_window::ResizableWindow,
};

use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use egui::{Color32, Shape, Stroke, StrokeKind};

use crate::syntax::tidal;
use crate::Pluguzu;
use crate::{highlights, presets};

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
    highlights: BTreeSet<Arc<highlights::EventLoc>>,
}

impl Pluguzu {
    pub fn do_editor(&mut self, async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let egui_state = params.editor_state.clone();
        let code = params.code.clone();
        let font_size = params.font_size.clone();
        let initial_font_size = font_size.load(Ordering::Relaxed);

        let dsp_state = self.dsp_state.clone();

        let current_pos = self.current_pos.clone();
        let evlocs = self.evlocs.clone();
        let evlocs_pos = self.evlocs_pos.clone();

        let state = UIState {
            view: UIView::Editor,
            preset: PresetState {
                cat: 0,
                selected: 0,
                presets: load_presets(),
            },
            highlights: BTreeSet::new(),
        };

        create_egui_editor(
            self.params.editor_state.clone(),
            state,
            move |egui_ctx, _| update_font_size(egui_ctx, initial_font_size),
            move |egui_ctx, _setter, state| {
                ResizableWindow::new("res-wind")
                    .min_size(Vec2::new(80.0, 60.0))
                    .show(egui_ctx, egui_state.as_ref(), |ui| {
                        // Main UI
                        match state.view {
                            UIView::Preset => {
                                if let Some(changed) = preset_ui(
                                    ui,
                                    &mut state.preset,
                                    font_size.load(Ordering::Relaxed),
                                ) {
                                    if let Some(new_code) = changed {
                                        *code.lock().unwrap() = new_code;
                                        async_executor
                                            .execute_background(crate::PluguzuTask::Parse(false));
                                        async_executor
                                            .execute_background(crate::PluguzuTask::RenderInPlace);
                                    }
                                    state.view = UIView::Editor;
                                };
                            }
                            UIView::Editor => {
                                let mut font_size_value = font_size.load(Ordering::Relaxed);
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Min),
                                    |ui| {
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
                                        if ui.button("Load").clicked() {
                                            state.view = UIView::Preset
                                        }
                                        let dsp_state = dsp_state.read().unwrap();
                                        let status = if dsp_state.playing { ">>=" } else { "|-|" };
                                        let next = dsp_state.next;
                                        let remaining = dsp_state.remaining;
                                        let error = dsp_state.error.as_deref().unwrap_or("OK");
                                        ui.label(RichText::new(format!(
                                            "{status} {next} | {remaining} events | {error}"
                                        )));
                                    },
                                );

                                let line_height = font_size_value as f32 + VERT_SPACE;
                                let avail_lines = (ui.available_height() / line_height).floor();

                                let mut code_editor = CodeEditor::default()
                                    .id_source("code editor")
                                    .with_rows(avail_lines as usize)
                                    .with_fontsize(font_size_value as f32)
                                    .with_theme(ColorTheme::SONOKAI)
                                    .with_syntax(tidal())
                                    .with_numlines(true);

                                let mut code_data = code.lock().unwrap();
                                let force_update = ui.input(|i| {
                                    i.key_pressed(egui::Key::Enter) && i.modifiers.shift
                                });
                                let output = if force_update {
                                    async_executor
                                        .execute_background(crate::PluguzuTask::RenderInPlace);
                                    // Render the code editor with read-only code to avoid inserting the new-line.
                                    let mut code_data: &str = code_data.deref();
                                    code_editor.show(ui, &mut code_data)
                                } else {
                                    code_editor.show(ui, code_data.deref_mut())
                                };

                                output.response.request_focus();

                                let painter = ui.painter_at(output.response.rect);
                                let ep = highlights::EditorPixels::new(
                                    output.galley_pos,
                                    &output.galley,
                                );

                                // Remove old highlights
                                let current_pos = current_pos.read().unwrap();
                                state
                                    .highlights
                                    .retain(|hl| current_pos.saturating_sub(hl.when) < 500);

                                // Add new highlights
                                let evlocs = evlocs.read().unwrap();
                                if dsp_state.read().unwrap().playing {
                                    let mut pos = evlocs_pos.load(Ordering::Relaxed) as usize;
                                    while pos < evlocs.len() {
                                        let evloc = &evlocs[pos];
                                        if *current_pos < evloc.when {
                                            break;
                                        }
                                        state.highlights.insert(evloc.clone());
                                        pos += 1;
                                    }
                                    evlocs_pos.store(pos as u32, Ordering::Relaxed);
                                }

                                let shapes = state.highlights.iter().map(|el| {
                                    let alpha = (egui::lerp(
                                        1.0..=0.3,
                                        (current_pos.saturating_sub(el.when) as f32) / 500.,
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

                                if !force_update && output.response.changed() {
                                    // TODO: sometime alt modifier get stuck after clicking, perhaps it could be manually removed to workaround this weird bug?
                                    // egui_ctx.input(|i| println!("HERE: {:?}", i.modifiers));
                                    async_executor
                                        .execute_background(crate::PluguzuTask::Parse(true))
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
