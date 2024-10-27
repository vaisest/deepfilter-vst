use nih_plug::nih_log;
use rtrb::RingBuffer;
use rubato::{FftFixedIn, FftFixedOut, Resampler};
use std::{
    ffi::{c_char, c_float, CString},
    hint::{self, spin_loop},
    thread,
};

type Sample = [f32; 2];

/// Wrap a `DfTrace` instance behind a worker thread. Note: this will add latency to the input.
//

pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

struct IOResampler {
    input: FftFixedOut<f32>,
    output: FftFixedIn<f32>,
}

#[repr(C)]
struct DFState {
    _private: [u8; 0],
}

#[link(name = "df")]
extern "C" {
    /// Create a DeepFilterNet Model
    fn df_create(path: *const c_char, atten_lim: f32) -> *mut DFState;
    /// Get DeepFilterNet frame size in samples.
    fn df_get_frame_length(state: *mut DFState) -> usize;
    /// Processes a chunk of samples.
    fn df_process_frame(
        state: *mut DFState,
        input: *const c_float,
        output: *mut c_float,
    ) -> c_float;
    /// Free a DeepFilterNet Model
    fn df_free(model: *mut DFState);
}

pub struct DeepFilter {
    state: *mut DFState,
}
impl DeepFilter {
    pub fn new() -> Self {
        let model_path = CString::new(
            "C:/Users/Turtvaiz/Downloads/deepfilter-vst/models/DeepFilterNet3_ll_onnx.tar.gz",
        )
        .expect("string broke");

        let state = unsafe { df_create(model_path.as_ptr(), 50.0) };

        return DeepFilter { state };
    }

    pub fn get_frame_length(&self) -> usize {
        unsafe { df_get_frame_length(self.state) as usize }
    }

    pub fn process_frame(&self, input: &[f32], output: &mut [f32]) -> f32 {
        debug_assert_eq!(input.len(), output.len());
        debug_assert_eq!(input.len(), self.get_frame_length());

        unsafe { df_process_frame(self.state, input.as_ptr(), output.as_mut_ptr()) }
    }
}

impl Drop for DeepFilter {
    fn drop(&mut self) {
        unsafe {
            df_free(self.state);
        }
    }
}

impl DfWrapper {
    pub fn new() -> Self {
        Self {
            sender: None,
            receiver: None,
            worker: None,
        }
    }

    fn nuke_and_annihilate_self(&mut self) {
        // kills worker and ring buffers
        nih_log!("nuked {:?}", thread::current().id());
        self.sender = None;
        self.receiver = None;
        self.worker = None;
    }

    pub fn init(&mut self, plugin_sample_rate: usize, plugin_buffer_len: usize) {
        self.nuke_and_annihilate_self();

        let buffer_size = 4096.max(plugin_buffer_len);

        // create two ring buffers: one for receiving samples from plugin, and another for sending them back
        // plugin_sender -> worker_input -> **worker processing** -> worker_sender -> worker_destination
        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(buffer_size);
        let (mut worker_sender, worker_destination) = RingBuffer::<Sample>::new(buffer_size);

        // Fill the initial buffer with zeroes
        nih_log!("sending {} zeroes...", buffer_size);
        for _ in 0..(buffer_size) {
            worker_sender.push([0.0; 2]).unwrap();
        }

        let worker = thread::spawn(move || {
            let model_sr = 48000;
            // capi filter doesn't seem to support multiple channels yet
            let left = DeepFilter::new();
            let right = DeepFilter::new();

            // let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
            //     .expect("init df failed");

            // todo: resampling optional when incoming sr is right
            let mut resampler = IOResampler {
                input: FftFixedOut::new(
                    plugin_sample_rate,
                    model_sr,
                    left.get_frame_length(),
                    1, // no clue what this subchunk thing is
                    2,
                )
                .expect("failed to create worker input resampler"),
                output: FftFixedIn::new(
                    model_sr,
                    plugin_sample_rate,
                    left.get_frame_length(),
                    1,
                    2,
                )
                .expect("failed to create worker output resampler"),
            };

            nih_log!(
                "worker thread {:?} initialised resampler, frames needed in {}",
                thread::current().id(),
                resampler.input.input_frames_next(),
            );

            // in_buf -> model_in_buf -> **model processing** -> model_out_buf -> out_buf
            // only in_buf is not filled by default as it has input samples appended to it
            // todo: fill it and use idx variable
            let mut in_buf = resampler.input.input_buffer_allocate(false);
            // resampler output has to already contain the amount of samples that will be output
            let mut model_in_buf = resampler.input.output_buffer_allocate(true);
            let mut model_out_buf = resampler.output.input_buffer_allocate(true);
            let mut out_buf = resampler.output.output_buffer_allocate(true);

            // model uses ndarray, reads from in, writes to mutable out
            let mut noisy = Array2::<f32>::zeros((2, model.hop_size));
            let mut enhanced = noisy.clone();

            nih_log!(
                "worker thread {:?} starting with model sr: {} and buffer size: {}",
                thread::current().id(),
                model.sr,
                buffer_size
            );

            // todo: signal that processing is ready to plugin thread

            // // as long as the ring buffer exists, poll for new data
            // while !worker_input.is_abandoned() {
            //     if worker_input.is_empty() {
            //         hint::spin_loop();
            //         continue;
            //     }

            //     let frame = worker_input.pop().unwrap();
            //     in_buf[0].push(frame[0]);
            //     in_buf[1].push(frame[1]);

            //     if in_buf[0].len() > resampler.input.input_frames_next() {
            //         // resample input, which should give us hop_size amount of samples in model_in_buf
            //         resampler
            //             .input
            //             .process_into_buffer(&in_buf, &mut model_in_buf, None)
            //             .expect("error while resampling input");
            //         in_buf[0].clear();
            //         in_buf[1].clear();

            //         // todo: iter for ndarrays

            //         // replace noisy with model_in_buf
            //         for c in 0..2 {
            //             for i in 0..model.hop_size {
            //                 noisy[[c, i]] = model_in_buf[c][i];
            //             }
            //         }

            //         model.process(noisy.view(), enhanced.view_mut()).unwrap();

            //         // replace model_out_buf with enhanced
            //         for c in 0..2 {
            //             for i in 0..model.hop_size {
            //                 model_out_buf[c][i] = enhanced[[c, i]];
            //             }
            //         }

            //         // resample output
            //         resampler
            //             .output
            //             .process_into_buffer(&model_out_buf, &mut out_buf, None)
            //             .expect("error while resampling output");

            //         for (&l, &r) in out_buf[0].iter().zip(out_buf[1].iter()) {
            //             // should not error as the same amount of samples was taken as input
            //             worker_sender.push([l, r]).unwrap();
            //         }
            //     }
            // }

            nih_log!("worker thread {:?} exiting...", thread::current().id());
        });

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        self.worker.replace(worker);
    }

    pub fn process(&mut self, sample: [&mut f32; 2]) -> Sample {
        // TODO: warn for long waits
        // TODO: variable channel count
        while self.receiver.as_mut().unwrap().is_empty() {
            spin_loop();
        }

        let out = self.receiver.as_mut().unwrap().pop().unwrap();

        self.sender
            .as_mut()
            .unwrap()
            .push([*sample[0], *sample[1]])
            .unwrap();
        *sample[0] = out[0];
        *sample[1] = out[1];
        return out;
    }
}
