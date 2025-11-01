// Copyright © 2025 Tristan de Cacqueray
// SPDX-License-Identifier: GPL-3.0

use crate::UzuKind;
use crate::egui;
use crate::egui::{
    Color32, FontId, TextFormat, style::ScrollStyle, text::LayoutJob, text_edit::TextEditOutput,
};

#[derive(Copy, Clone)]
struct FontSize(f32);
impl std::hash::Hash for FontSize {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        ((self.0 * 1000.0) as u64).hash(state)
    }
}

fn highlight(ctx: &egui::Context, fontsize: f32, lang: UzuKind, code: &str) -> LayoutJob {
    #[expect(non_local_definitions)]
    impl egui::cache::ComputerMut<(FontSize, UzuKind, &str), LayoutJob> for Highlighter {
        fn compute(&mut self, (fontsize, lang, code): (FontSize, UzuKind, &str)) -> LayoutJob {
            Self::highlight(fontsize.0, lang, code)
        }
    }

    ctx.memory_mut(|mem| {
        mem.caches
            .cache::<egui::cache::FrameCache<LayoutJob, Highlighter>>()
            .get((FontSize(fontsize), lang, code))
    })
}

#[derive(Default)]
struct Highlighter;
impl Highlighter {
    fn highlight(fontsize: f32, _lang: UzuKind, code: &str) -> LayoutJob {
        let mut job = LayoutJob::default();
        let font_id = FontId::monospace(fontsize);
        // TODO: highlight the syntax!
        job.append(
            code,
            0.,
            TextFormat::simple(font_id.clone(), Color32::from_rgb(0xe2, 0xe2, 0xe3)),
        );
        job
    }
}

pub fn show(
    ui: &mut egui::Ui,
    fontsize: f32,
    lang: UzuKind,
    code: &mut String,
    rows: usize,
) -> TextEditOutput {
    let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, _wrap_width: f32| {
        let layout_job = highlight(ui.ctx(), fontsize, lang, buf.as_str());
        ui.fonts(|f| f.layout_job(layout_job))
    };
    ui.spacing_mut().scroll = ScrollStyle::solid();
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .show(ui, |ui| {
                    egui::TextEdit::multiline(code)
                        .desired_rows(rows)
                        .lock_focus(true)
                        .desired_width(f32::INFINITY)
                        .background_color(Color32::from_rgb(0x2c, 0x2e, 0x34))
                        .frame(true)
                        .layouter(&mut layouter)
                        .show(ui)
                })
                .inner
        })
        .inner
}
