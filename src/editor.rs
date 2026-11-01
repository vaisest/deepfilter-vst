use crate::VstParams;
use egui::{CentralPanel, Frame, Margin, RichText};
use nice_plug::{context::gui::GuiContext, editor::dpi::LogicalSize, prelude::*};
use nice_plug_egui::{
    create_egui_editor, widgets::ParamSlider, EguiEditor, EguiEditorState, EguiNiceSettings,
    NiceEguiApp, RepaintNotifier,
};
use std::sync::Arc;

const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(360.0, 340.0);

pub fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(WINDOW_SIZE, 1.0)
}

pub fn create(
    params: Arc<VstParams>,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
) -> Option<EguiEditor<VstEditor>> {
    // the default resize hint makes the window fixed size
    create_egui_editor(
        editor_state,
        repaint_notifier,
        EguiNiceSettings::new(),
        VstEditor {
            params,
            gui_context: None,
        },
    )
}

pub struct VstEditor {
    params: Arc<VstParams>,
    /// only exists while the editor window is open
    gui_context: Option<GuiContext>,
}

impl NiceEguiApp for VstEditor {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        // the default tooltip width is wider than the window, which cuts tooltips off
        egui_ctx.all_styles_mut(|style| style.spacing.tooltip_width = WINDOW_SIZE.width - 60.0);
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn editor_closed(&mut self) {
        self.gui_context = None;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = &self.gui_context else {
            return;
        };
        let setter = gui_context.param_setter();
        let p = &self.params;
        // (param, what it does, what changing it does in practice)
        let sliders: [(&FloatParam, &str, &str); 5] = [
            (
                &p.attenuation_limit,
                "The maximum amount the noise is reduced by. 100 dB means no limit.",
                "Lower it if the voice sounds robotic or muffled, or if you want to keep \
                 some natural background sound. Around 20-40 dB is a softer reduction.",
            ),
            (
                &p.min_thresh,
                "Moments where the estimated signal-to-noise ratio is below this are \
                 treated as pure noise and silenced.",
                "Raise it to make pauses between words quieter, like a noise gate. Too \
                 high and quiet speech or the ends of words may get cut off.",
            ),
            (
                &p.max_erb_thresh,
                "Moments where the estimated signal-to-noise ratio is above this are \
                 considered clean and passed through untouched.",
                "Lower it to leave loud, clear speech completely unprocessed for a more \
                 natural sound. Too low and noise under your voice is no longer removed.",
            ),
            (
                &p.max_df_thresh,
                "Moments where the estimated signal-to-noise ratio is above this skip the \
                 second, finer filtering stage.",
                "Lower it to save CPU if you hear crackling or dropouts. The cost is \
                 slightly less clean speech when there is a lot of noise.",
            ),
            (
                &p.post_filter_beta,
                "Strength of an extra filter that further reduces leftover noise. \
                 0 disables it.",
                "Small values like 0.02 remove a bit more noise between words, but can \
                 make the voice sound thinner.",
            ),
        ];

        CentralPanel::default()
            .frame(Frame::central_panel(ui.style()).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                ui.heading("DeepFilterNet");
                // leave room for the value text drawn after the slider
                let slider_width = ui.available_width() - 80.0;
                for (param, what, in_practice) in sliders {
                    // tooltips containing technical and practical explanations
                    let hint = |ui: &mut egui::Ui| {
                        ui.label(what);
                        ui.add_space(4.0);
                        ui.label(RichText::new(format!("In practice: {in_practice}")).weak());
                    };
                    ui.add_space(8.0);
                    ui.label(param.name()).on_hover_ui(hint);
                    ui.add(ParamSlider::for_param(param, &setter).with_width(slider_width))
                        .on_hover_ui(hint);
                }
            });
    }
}
