use nih_plug::prelude::*;
use std::sync::Arc;
mod thread;

pub struct Vst {
    model: thread::DfWrapper,
    params: Arc<VstParams>,
}

#[derive(Params)]
struct VstParams {
    /// The parameter's ID is used to identify the parameter in the wrappred plugin API. As long as
    /// these IDs remain constant, you can rename and reorder these fields as you wish. The
    /// parameters are exposed to the host in the same order they were defined. In this case, this
    /// gain parameter is stored as linear gain while the values are displayed in decibels.
    #[id = "attenuation_limit"]
    pub attenuation_limit: FloatParam,
    // pub min_thresh: FloatParam,
    // pub max_erb: FloatParam,
    // pub max_thresh: FloatParam,

    // /// "We adopt the post-filter, first proposed by Valin et al., with the
    // /// aim of slightly over-attenuating noisy TF bins while adding some gain
    // /// back to less noisy bins.""
    // pub post_filter_beta: FloatParam,
}

impl Default for Vst {
    fn default() -> Self {
        Self {
            model: thread::DfWrapper::new(70.),
            params: Arc::new(VstParams::default()),
        }
    }
}

impl Default for VstParams {
    fn default() -> Self {
        Self {
            // TODO: implement model parameters here settings
            attenuation_limit: FloatParam::new(
                "Attenuation Limit",
                util::db_to_gain(70.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(0.01),
                    max: util::db_to_gain(100.0),
                    // This makes the range appear as if it was linear when displaying the values as
                    // decibels
                    factor: FloatRange::gain_skew_factor(0.01, 100.0),
                },
            )
            // // Because the gain parameter is stored as linear gain instead of storing the value as
            // // decibels, we need logarithmic smoothing
            // .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            // There are many predefined formatters we can use here. If the gain was stored as
            // decibels instead of as a linear gain value, we could have also used the
            // `.with_step_size(0.1)` function to get internal rounding.
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for Vst {
    const NAME: &'static str = "deepfilter-vst";
    const VENDOR: &'static str = "vaisest";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "dont@email.me";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // The first audio IO layout is used as the default. The other layouts may be selected either
    // explicitly or automatically by the host or the user depending on the plugin API/backend.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),

        aux_input_ports: &[],
        aux_output_ports: &[],

        // Individual ports and the layout as a whole can be named here. By default these names
        // are generated as needed. This layout will be called 'Stereo', while a layout with
        // only one input and output channel would be called 'Mono'.
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

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        nih_log!(
            "buffer_size: {:?}-{:?}",
            buffer_config.min_buffer_size,
            buffer_config.max_buffer_size
        );

        nih_log!("plugin sr: {}", buffer_config.sample_rate);

        self.model.init(buffer_config.sample_rate as usize);

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        self.model
            .update_atten_limit(self.params.attenuation_limit.value());
        // could probably use iter_blocks instead?
        for channel_samples in buffer.iter_samples() {
            let mut it = channel_samples.into_iter();

            self.model.process([it.next().unwrap(), it.next().unwrap()]);
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        nih_log!("deactivated lol");
    }
}

impl Vst3Plugin for Vst {
    const VST3_CLASS_ID: [u8; 16] = *b"fooofooofooofooo";

    // And also don't forget to change these categories
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

impl ClapPlugin for Vst {
    const CLAP_ID: &'static str = "deepfilternetvst";
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Filter];
    const CLAP_DESCRIPTION: Option<&'static str> = None;
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
}

nih_export_vst3!(Vst);
nih_export_clap!(Vst);
