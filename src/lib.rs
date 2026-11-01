use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditor, EguiEditorState, RepaintNotifier};
use std::sync::Arc;
mod editor;
mod thread;

pub struct Vst {
    model: thread::DfWrapper,
    params: Arc<VstParams>,
    editor_state: Arc<EguiEditorState>,
    repaint_notifier: RepaintNotifier,
}

#[derive(Params)]
struct VstParams {
    /// The parameter's ID is used to identify the parameter in the wrappred plugin API. As long as
    /// these IDs remain constant, you can rename and reorder these fields as you wish. The
    /// parameters are exposed to the host in the same order they were defined. In this case, this
    /// gain parameter is stored as linear gain while the values are displayed in decibels.
    #[id = "attenuation_limit"]
    pub attenuation_limit: FloatParam,
    /// Frames with a local SNR below this are treated as noise only and fully attenuated
    #[id = "min_thresh"]
    pub min_thresh: FloatParam,
    /// Frames with a local SNR above this are passed through without processing
    #[id = "max_erb_thresh"]
    pub max_erb_thresh: FloatParam,
    /// Frames with a local SNR above this skip the deep filtering stage and only get the
    /// ERB gains applied
    #[id = "max_df_thresh"]
    pub max_df_thresh: FloatParam,
    /// "We adopt the post-filter, first proposed by Valin et al., with the
    /// aim of slightly over-attenuating noisy TF bins while adding some gain
    /// back to less noisy bins." 0 disables the post-filter.
    #[id = "post_filter_beta"]
    pub post_filter_beta: FloatParam,
}

impl VstParams {
    fn model_settings(&self) -> thread::ModelSettings {
        thread::ModelSettings {
            atten_lim: self.attenuation_limit.value(),
            min_db_thresh: self.min_thresh.value(),
            max_db_erb_thresh: self.max_erb_thresh.value(),
            max_db_df_thresh: self.max_df_thresh.value(),
            post_filter_beta: self.post_filter_beta.value(),
        }
    }
}

/// A parameter in decibels that accepts both "50" and "50 dB" as input
fn db_param(name: &str, default: f32, min: f32, max: f32, step: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min, max })
        .with_unit(" dB")
        .with_step_size(step)
        .with_value_to_string(formatters::v2s_f32_rounded(2))
        .with_string_to_value(Arc::new(|s| {
            s.trim()
                .trim_end_matches("dB")
                .trim_end()
                .parse::<f32>()
                .ok()
        }))
}

impl Default for Vst {
    fn default() -> Self {
        let params = Arc::new(VstParams::default());
        Self {
            model: thread::DfWrapper::new(params.model_settings()),
            params,
            editor_state: editor::default_state(),
            repaint_notifier: RepaintNotifier::new(),
        }
    }
}

impl Default for VstParams {
    fn default() -> Self {
        // threshold ranges and defaults are the same as in the official LADSPA plugin
        Self {
            attenuation_limit: db_param("Attenuation Limit", 70.0, 0.1, 100.0, 0.1),
            min_thresh: db_param("Min Processing Threshold", -15.0, -15.0, 35.0, 0.5),
            max_erb_thresh: db_param("Max ERB Processing Threshold", 35.0, -15.0, 35.0, 0.5),
            max_df_thresh: db_param("Max DF Processing Threshold", 35.0, -15.0, 35.0, 0.5),
            post_filter_beta: FloatParam::new(
                "Post Filter Beta",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 0.05,
                },
            )
            .with_step_size(0.001)
            .with_value_to_string(formatters::v2s_f32_rounded(3)),
        }
    }
}

impl Plugin for Vst {
    const NAME: &'static str = "deepfilter-vst";
    const VENDOR: &'static str = "vaisest";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "dont@email.me";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // This doesn't seem to require a mono layout for audacity. I'm unsure if
    // this is the case in other software.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),

        aux_input_ports: &[],
        aux_output_ports: &[],

        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = false;

    // If the plugin can send or receive SysEx messages, it can define a type to wrap around those
    // messages here. The type implements the `SysExMessage` trait, which allows conversion to and
    // from plain byte buffers.
    type SysExMessage = ();
    // More advanced plugins can use this to run expensive background tasks. See the field's
    // documentation for more information. `()` means that the plugin does not have any background
    // tasks.
    type BackgroundTask = ();
    type Editor = EguiEditor<editor::VstEditor>;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        editor::create(
            self.params.clone(),
            self.editor_state.clone(),
            self.repaint_notifier.clone(),
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        nice_log!(
            "buffer_size: {:?}-{:?}",
            buffer_config.min_buffer_size,
            buffer_config.max_buffer_size
        );

        let settings = self.params.model_settings();
        nice_log!(
            "plugin sr: {}, model settings: {:?}",
            buffer_config.sample_rate,
            settings
        );

        self.model.update_settings(settings);
        let Some(latency) = self.model.init(buffer_config.sample_rate as usize) else {
            nice_log!("the model failed to start");
            return false;
        };
        context.set_latency_samples(latency);

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.model.update_settings(self.params.model_settings());
        // could probably use iter_blocks instead?
        for channel_samples in buffer.iter_samples() {
            let mut it = channel_samples.into_iter();

            self.model.process([it.next().unwrap(), it.next().unwrap()]);
        }
        self.model.wake_worker();

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        self.model.stop();
    }
}

impl Vst3Plugin for Vst {
    const VST3_CLASS_ID: [u8; 16] = *b"deepfilter-vst__";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Dynamics,
        Vst3SubCategory::Filter,
        Vst3SubCategory::Fx,
    ];
}

impl ClapPlugin for Vst {
    const CLAP_ID: &'static str = "deepfilternetvst";
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Filter];
    const CLAP_DESCRIPTION: Option<&'static str> = None;
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
}

nice_export_vst3!(Vst);
nice_export_clap!(Vst);
