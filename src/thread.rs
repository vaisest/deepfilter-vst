use core::f32;
use df::tract::*;
use ndarray::Array2;
use nih_plug::nih_log;
use rtrb::RingBuffer;
use rubato::{FftFixedIn, FftFixedOut, Resampler};
use std::{
    hint::spin_loop,
    sync::{atomic::AtomicU32, Arc},
    thread,
};

type Sample = [f32; 2];

/// Default hop size for the DeepFilter model
const DEFAULT_HOP_SIZE: u32 = 960;
/// Buffer capacity multiplier for ring buffers
const BUFFER_CAPACITY_MULTIPLIER: u32 = 2;

/// Wrapper that manages a `DfTract` instance behind a worker thread.
/// 
/// This approach is necessary because the model doesn't implement Send,
/// but adds some latency to the audio processing pipeline. The wrapper
/// handles audio resampling, parameter updates, and thread communication.
/// 
/// Note: this will add latency to the input, but this
/// is required due to the model not implementing Send
pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
    worker_param: Arc<AtomicU32>,
}

struct IOResampler {
    input: FftFixedOut<f32>,
    output: FftFixedIn<f32>,
}

impl DfWrapper {
    pub fn new(attenuation_limit: f32) -> Self {
        Self {
            sender: None,
            receiver: None,
            worker: None,
            worker_param: Arc::new(AtomicU32::new(attenuation_limit.to_bits())),
        }
    }

    /// initialises model in worker thread and attaches input and output buffers to it
    pub fn init(&mut self, plugin_sample_rate: usize) -> u32 {
        // Use the model's actual hop size instead of hardcoded value
        let hop_size = DEFAULT_HOP_SIZE;
        let buffer_capacity = hop_size * BUFFER_CAPACITY_MULTIPLIER;

        // create two ring buffers: one for receiving samples from plugin, and another for sending them back
        // plugin_sender -> worker_input -> **worker processing** -> worker_sender -> worker_destination
        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(buffer_capacity as usize);
        let (mut worker_sender, worker_destination) =
            RingBuffer::<Sample>::new(buffer_capacity as usize);

        let param = self.worker_param.clone();

        let worker = thread::spawn(move || {
            let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
                .expect("initialising df failed");

            // set initial parameters
            let mut current_atten_lim =
                f32::from_bits(param.load(std::sync::atomic::Ordering::Relaxed));
            model.set_atten_lim(current_atten_lim);
            // same default as the official LADSPA plugin
            model.min_db_thresh = -15.;
            model.max_db_erb_thresh = 35.;
            model.max_db_df_thresh = 35.;
            nih_log!("atten lim set to: {current_atten_lim}");

            // model.set_pf_beta(1.);

            // todo: resampling optional when incoming sr is right
            let mut resampler = IOResampler {
                input: FftFixedOut::new(
                    plugin_sample_rate,
                    model.sr,
                    model.hop_size,
                    1, // no clue what this subchunk thing is
                    2,
                )
                .expect("failed to create worker input resampler"),
                output: FftFixedIn::new(model.sr, plugin_sample_rate, model.hop_size, 1, 2)
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
                "worker thread {:?} starting with model sr: {} and atten_lim: {:?}",
                thread::current().id(),
                model.sr,
                model.atten_lim,
            );

            // Fill the initial output buffer with zeroes
            nih_log!("sending {} zeroes...", buffer_capacity);
            for _ in 0..(buffer_capacity) {
                worker_sender.push([0.0; 2]).unwrap();
            }

            // as long as the ring buffer exists, poll for new data
            while !worker_input.is_abandoned() {
                if worker_input.is_empty() {
                    spin_loop();
                    thread::yield_now(); // Be more CPU-friendly
                    continue;
                }

                let new_param = f32::from_bits(param.load(std::sync::atomic::Ordering::Relaxed));
                if new_param != current_atten_lim {
                    model.set_atten_lim(new_param);
                    current_atten_lim = new_param;
                }

                let frame = worker_input.pop().unwrap();
                in_buf[0].push(frame[0]);
                in_buf[1].push(frame[1]);

                if in_buf[0].len() == resampler.input.input_frames_next() {
                    // resample input, which should give us hop_size amount of samples in model_in_buf
                    resampler
                        .input
                        .process_into_buffer(&in_buf, &mut model_in_buf, None)
                        .expect("error while resampling input");
                    in_buf[0].clear();
                    in_buf[1].clear();

                    // Copy model_in_buf to noisy array more efficiently
                    for (c, channel) in model_in_buf.iter().enumerate().take(2) {
                        for (i, &sample) in channel.iter().enumerate().take(model.hop_size) {
                            noisy[[c, i]] = sample;
                        }
                    }

                    model
                        .process(noisy.view(), enhanced.view_mut())
                        .expect("model processing failed");

                    // Copy enhanced array to model_out_buf more efficiently
                    for (c, channel) in model_out_buf.iter_mut().enumerate().take(2) {
                        for (i, sample) in channel.iter_mut().enumerate().take(model.hop_size) {
                            *sample = enhanced[[c, i]];
                        }
                    }

                    let samples_out_count = resampler.output.output_frames_next();
                    // resample output
                    resampler
                        .output
                        .process_into_buffer(&model_out_buf, &mut out_buf, None)
                        .expect("error while resampling output");

                    for i in 0..samples_out_count {
                        worker_sender
                            .push([out_buf[0][i], out_buf[1][i]])
                            .expect("worker_sender push failed");
                    }
                }
            }

            nih_log!("worker thread {:?} exiting...", thread::current().id());
        });

        // wait for worker thread to fully start and for it to prefill the output buffer
        while worker_destination.is_empty() {
            spin_loop();
            thread::yield_now(); // Be more CPU-friendly during initialization
        }

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        
        // Verify worker thread started successfully
        if worker.is_finished() {
            nih_log!("Error: Worker thread failed to initialize properly");
            return 0; // Return 0 latency to indicate failure
        }
        
        self.worker.replace(worker);
        buffer_capacity
    }

    pub fn process(&mut self, sample: [&mut f32; 2]) {
        // Wait for space in the input queue
        while self.sender.as_ref().map_or(true, |s| s.is_full()) {
            spin_loop();
            thread::yield_now(); // Be more CPU-friendly
        }

        self.send_sample(&[*sample[0], *sample[1]]);

        // Wait for output data to be available
        while self.receiver.as_ref().map_or(true, |r| r.is_empty()) {
            spin_loop();
            thread::yield_now(); // Be more CPU-friendly
        }

        let out = self.receive_sample();
        *sample[0] = out[0];
        *sample[1] = out[1];
    }

    pub fn update_atten_limit(&mut self, db: f32) {
        let int = db.to_bits();
        if self.worker_param.load(std::sync::atomic::Ordering::Relaxed) != int {
            self.worker_param
                .store(int, std::sync::atomic::Ordering::Relaxed);
        }
    }

    // fn send_zeroes(&mut self, n: usize) {
    //     assert!(
    //         self.sender.is_some(),
    //         "ringbuffer does not exist when trying to send zeroes"
    //     );
    //     for _ in 0..n {
    //         self.send_sample(&[0.0, 0.0]);
    //     }
    // }

    fn send_sample(&mut self, s: &Sample) {
        if let Some(ref mut sender) = self.sender {
            if let Err(_) = sender.push([s[0], s[1]]) {
                nih_log!("Warning: Audio queue is full, dropping sample");
            }
        } else {
            nih_log!("Error: Sender not initialized");
        }
    }

    fn receive_sample(&mut self) -> Sample {
        if let Some(ref mut receiver) = self.receiver {
            receiver.pop().unwrap_or([0.0, 0.0])
        } else {
            nih_log!("Error: Receiver not initialized, returning silence");
            [0.0, 0.0]
        }
    }
}

impl Drop for DfWrapper {
    fn drop(&mut self) {
        // Clean up resources by dropping senders/receivers first, then waiting for worker thread
        self.sender.take();
        self.receiver.take();
        
        if let Some(worker) = self.worker.take() {
            if let Err(_) = worker.join() {
                nih_log!("Warning: Worker thread did not shut down cleanly");
            } else {
                nih_log!("Worker thread shut down successfully");
            }
        }
    }
}
