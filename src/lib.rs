use nih_plug::prelude::*;
use std::sync::Arc;
mod thread;

/// VST plugin implementation for DeepFilter noise reduction.
/// 
/// This plugin uses the DeepFilter neural network model to reduce noise in audio signals.
/// It processes audio in a separate worker thread to avoid blocking the audio processing thread.
pub struct Vst {
    model: thread::DfWrapper,
    params: Arc<VstParams>,
    last_attenuation_limit: f32,
    last_min_thresh: f32,
    last_max_erb: f32,
    last_max_thresh: f32,
    last_post_filter_beta: f32,
}

#[derive(Params)]
struct VstParams {
    /// Controls the maximum attenuation applied to noisy frequency bins.
    /// Higher values allow more aggressive noise reduction but may affect speech quality.
    /// The parameter's ID is used to identify the parameter in the wrapped plugin API. As long as
    /// these IDs remain constant, you can rename and reorder these fields as you wish. The
    /// parameters are exposed to the host in the same order they were defined.
    #[id = "attenuation_limit"]
    pub attenuation_limit: FloatParam,
    
    /// Controls the minimum threshold for noise detection.
    /// Lower values make the filter more sensitive to noise but may affect quiet speech.
    #[id = "min_thresh"]
    pub min_thresh: FloatParam,
    
    /// Controls the maximum ERB (Equivalent Rectangular Bandwidth) threshold.
    /// Affects frequency-domain processing sensitivity.
    #[id = "max_erb"]
    pub max_erb: FloatParam,
    
    /// Controls the maximum threshold for DeepFilter processing.
    /// Higher values allow more aggressive processing.
    #[id = "max_thresh"]
    pub max_thresh: FloatParam,
    
    /// Controls the post-filter beta coefficient.
    /// Affects the strength of post-processing filtering.
    #[id = "post_filter_beta"]
    pub post_filter_beta: FloatParam,
}

impl Default for Vst {
    fn default() -> Self {
        Self {
            model: thread::DfWrapper::new(70., -15., 35., 35., 1.),
            params: Arc::new(VstParams::default()),
            last_attenuation_limit: 70.0,
            last_min_thresh: -15.0,
            last_max_erb: 35.0,
            last_max_thresh: 35.0,
            last_post_filter_beta: 1.0,
        }
    }
}

impl Default for VstParams {
    fn default() -> Self {
        Self {
            attenuation_limit: FloatParam::new(
                "Attenuation Limit",
                70.0,
                FloatRange::Linear {
                    min: 0.1,
                    max: 100.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(2))
            .with_string_to_value(Arc::new(|s| {
                if let Some((n, _)) = s.split_once(" ") {
                    n.parse::<f32>().ok()
                } else {
                    None
                }
            })),
            min_thresh: FloatParam::new(
                "Min Threshold",
                -15.0,
                FloatRange::Linear {
                    min: -30.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(Arc::new(|s| {
                if let Some((n, _)) = s.split_once(" ") {
                    n.parse::<f32>().ok()
                } else {
                    None
                }
            })),
            max_erb: FloatParam::new(
                "Max ERB Threshold",
                35.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 50.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(Arc::new(|s| {
                if let Some((n, _)) = s.split_once(" ") {
                    n.parse::<f32>().ok()
                } else {
                    None
                }
            })),
            max_thresh: FloatParam::new(
                "Max Threshold",
                35.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 50.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_value_to_string(formatters::v2s_f32_rounded(1))
            .with_string_to_value(Arc::new(|s| {
                if let Some((n, _)) = s.split_once(" ") {
                    n.parse::<f32>().ok()
                } else {
                    None
                }
            })),
            post_filter_beta: FloatParam::new(
                "Post Filter Beta",
                1.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 2.0,
                },
            )
            .with_step_size(0.01)
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
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
    // TODO: add mono
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
        context: &mut impl InitContext<Self>,
    ) -> bool {
        nih_log!(
            "buffer_size: {:?}-{:?}",
            buffer_config.min_buffer_size,
            buffer_config.max_buffer_size
        );

        nih_log!(
            "plugin sr: {}, plugin params: {:?}",
            buffer_config.sample_rate,
            self.params.attenuation_limit
        );

        let latency = self.model.init(buffer_config.sample_rate as usize);
        context.set_latency_samples(latency);
        
        // Initialize cached parameter values and update model
        self.last_attenuation_limit = self.params.attenuation_limit.value();
        self.last_min_thresh = self.params.min_thresh.value();
        self.last_max_erb = self.params.max_erb.value();
        self.last_max_thresh = self.params.max_thresh.value();
        self.last_post_filter_beta = self.params.post_filter_beta.value();
        
        self.model.update_atten_limit(self.last_attenuation_limit);
        self.model.update_min_thresh(self.last_min_thresh);
        self.model.update_max_erb(self.last_max_erb);
        self.model.update_max_thresh(self.last_max_thresh);
        self.model.update_post_filter_beta(self.last_post_filter_beta);

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Check and update all parameters if they have changed
        let current_attenuation_limit = self.params.attenuation_limit.value();
        if (current_attenuation_limit - self.last_attenuation_limit).abs() > f32::EPSILON {
            self.model.update_atten_limit(current_attenuation_limit);
            self.last_attenuation_limit = current_attenuation_limit;
        }
        
        let current_min_thresh = self.params.min_thresh.value();
        if (current_min_thresh - self.last_min_thresh).abs() > f32::EPSILON {
            self.model.update_min_thresh(current_min_thresh);
            self.last_min_thresh = current_min_thresh;
        }
        
        let current_max_erb = self.params.max_erb.value();
        if (current_max_erb - self.last_max_erb).abs() > f32::EPSILON {
            self.model.update_max_erb(current_max_erb);
            self.last_max_erb = current_max_erb;
        }
        
        let current_max_thresh = self.params.max_thresh.value();
        if (current_max_thresh - self.last_max_thresh).abs() > f32::EPSILON {
            self.model.update_max_thresh(current_max_thresh);
            self.last_max_thresh = current_max_thresh;
        }
        
        let current_post_filter_beta = self.params.post_filter_beta.value();
        if (current_post_filter_beta - self.last_post_filter_beta).abs() > f32::EPSILON {
            self.model.update_post_filter_beta(current_post_filter_beta);
            self.last_post_filter_beta = current_post_filter_beta;
        }

        // Process audio samples
        for channel_samples in buffer.iter_samples() {
            let mut it = channel_samples.into_iter();
            if let (Some(left), Some(right)) = (it.next(), it.next()) {
                self.model.process([left, right]);
            }
        }

        ProcessStatus::Normal
    }

    fn deactivate(&mut self) {
        nih_log!("Plugin deactivated");
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
