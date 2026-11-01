use audio_thread_priority::{
    demote_current_thread_from_real_time, promote_current_thread_to_real_time,
};
use audioadapter::{Adapter, AdapterMut};
use audioadapter_buffers::{direct::SequentialSliceOfVecs, owned::SequentialOwned};
use core::f32;
use df::tract::*;
use ndarray::Array2;
use nice_plug::nice_log;
use rtrb::RingBuffer;
use rubato::{Fft, Resampler};
use std::{
    hint::spin_loop,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    thread,
};

type Sample = [f32; 2];

/// Model parameters that can be changed while the worker thread is running
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ModelSettings {
    pub atten_lim: f32,
    pub min_db_thresh: f32,
    pub max_db_erb_thresh: f32,
    pub max_db_df_thresh: f32,
    pub post_filter_beta: f32,
}

impl ModelSettings {
    fn apply(&self, model: &mut DfTract) {
        model.set_atten_lim(self.atten_lim);
        model.min_db_thresh = self.min_db_thresh;
        model.max_db_erb_thresh = self.max_db_erb_thresh;
        model.max_db_df_thresh = self.max_db_df_thresh;
        model.set_pf_beta(self.post_filter_beta);
    }
}

/// `ModelSettings` stored as f32 bits so it can be shared with the worker without locking.
/// Fields are updated individually, but a partially updated read is corrected on the next hop.
struct SharedSettings([AtomicU32; 5]);

impl SharedSettings {
    fn new(settings: ModelSettings) -> Self {
        let s = Self(Default::default());
        s.store(settings);
        s
    }

    fn store(&self, settings: ModelSettings) {
        let values = [
            settings.atten_lim,
            settings.min_db_thresh,
            settings.max_db_erb_thresh,
            settings.max_db_df_thresh,
            settings.post_filter_beta,
        ];
        for (atomic, value) in self.0.iter().zip(values) {
            atomic.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    fn load(&self) -> ModelSettings {
        let [atten_lim, min_db_thresh, max_db_erb_thresh, max_db_df_thresh, post_filter_beta] =
            self.0
                .each_ref()
                .map(|a| f32::from_bits(a.load(Ordering::Relaxed)));
        ModelSettings {
            atten_lim,
            min_db_thresh,
            max_db_erb_thresh,
            max_db_df_thresh,
            post_filter_beta,
        }
    }
}

/// Wrap a `DfTract` instance behind a worker thread.
/// Note: this will add latency to the input, but this
/// is required due to the model not implementing Send
pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// set from the audio thread when it notices the worker is gone, so that it doesn't wait on it
    worker_died: bool,
    settings: Arc<SharedSettings>,
}

struct IOResampler {
    input: Fft<f32>,
    output: Fft<f32>,
}

impl DfWrapper {
    pub fn new(settings: ModelSettings) -> Self {
        Self {
            sender: None,
            receiver: None,
            worker: None,
            worker_died: false,
            settings: Arc::new(SharedSettings::new(settings)),
        }
    }

    /// initialises model in worker thread and attaches input and output buffers to it. Returns the
    /// latency in samples, or `None` if the worker failed to start.
    pub fn init(&mut self, plugin_sample_rate: usize) -> Option<u32> {
        self.stop();

        // the field is private, but we know it's 480
        let hop_size = 960u32;
        let buffer_capacity = hop_size * 2;

        // create two ring buffers: one for receiving samples from plugin, and another for sending them back
        // plugin_sender -> worker_input -> **worker processing** -> worker_sender -> worker_destination
        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(buffer_capacity as usize);
        let (mut worker_sender, worker_destination) =
            RingBuffer::<Sample>::new(buffer_capacity as usize);

        let shared_settings = self.settings.clone();

        let worker = thread::spawn(move || {
            let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
                .expect("initialising df failed");

            // set initial parameters
            let mut current_settings = shared_settings.load();
            current_settings.apply(&mut model);
            nice_log!("model settings: {current_settings:?}");

            // todo: resampling optional when incoming sr is right
            let mut resampler = IOResampler {
                input: Fft::<f32>::new(
                    plugin_sample_rate,
                    model.sr,
                    model.hop_size,
                    1, // "It is recommended to set sub_chunks to 1 unless this leads to an unacceptably large delay."
                    2,
                    rubato::FixedSync::Output,
                )
                .expect("input resampler should have been initialised"),
                output: Fft::<f32>::new(
                    model.sr,
                    plugin_sample_rate,
                    model.hop_size,
                    1,
                    2,
                    rubato::FixedSync::Input,
                )
                .expect("output resampler should have been initialised"),
            };

            // in_buf -> **resample** -> model_in_buf -> **model processing** -> model_out_buf -> **resample** -> out_buf
            let mut in_buf = [
                Vec::with_capacity(resampler.input.input_frames_max()),
                Vec::with_capacity(resampler.input.input_frames_max()),
            ];
            let mut model_in_buf = SequentialOwned::new(0.0f32, 2, model.hop_size);
            let mut model_out_buf = SequentialOwned::new(0.0f32, 2, model.hop_size);
            let mut out_buf = SequentialOwned::new(0.0f32, 2, resampler.output.output_frames_max());

            // model uses ndarray, reads from in, writes to mutable out
            let mut noisy = Array2::<f32>::zeros((2, model.hop_size));
            let mut enhanced = noisy.clone();

            nice_log!(
                "worker thread {:?} starting with model sr: {}",
                thread::current().id(),
                model.sr,
            );

            // use the same scheduling class as audio threads, so that the worker isn't preempted
            // by regular threads when the system is busy.
            let rt_handle =
                match promote_current_thread_to_real_time(model.hop_size as u32, model.sr as u32) {
                    Ok(handle) => Some(handle),
                    Err(e) => {
                        nice_log!("could not raise worker thread priority: {e}");
                        None
                    }
                };

            // Fill the initial output buffer with zeroes
            nice_log!("sending {} zeroes...", buffer_capacity);
            for _ in 0..(buffer_capacity) {
                worker_sender.push([0.0; 2]).unwrap();
            }

            // as long as the ring buffer exists, process new data
            while !worker_input.is_abandoned() {
                let Ok(frame) = worker_input.pop() else {
                    // sleep until the plugin sends more samples
                    thread::park();
                    continue;
                };
                in_buf[0].push(frame[0]);
                in_buf[1].push(frame[1]);

                if in_buf[0].len() == resampler.input.input_frames_next() {
                    // resample input, which should give us hop_size amount of samples in model_in_buf
                    let input_adapter =
                        SequentialSliceOfVecs::new(&in_buf, 2, in_buf[0].len()).unwrap();
                    resampler
                        .input
                        .process_into_buffer(&input_adapter, &mut model_in_buf, None)
                        .expect("error while resampling input");
                    in_buf[0].clear();
                    in_buf[1].clear();

                    // replace noisy with model_in_buf
                    for c in 0..2 {
                        for i in 0..model.hop_size {
                            noisy[[c, i]] = model_in_buf.read_sample(c, i).unwrap();
                        }
                    }

                    // pick up parameter changes once per hop
                    let new_settings = shared_settings.load();
                    if new_settings != current_settings {
                        new_settings.apply(&mut model);
                        current_settings = new_settings;
                    }

                    model
                        .process(noisy.view(), enhanced.view_mut())
                        .expect("model processing failed");

                    // replace model_out_buf with enhanced
                    for c in 0..2 {
                        for i in 0..model.hop_size {
                            model_out_buf.write_sample(c, i, &enhanced[[c, i]]);
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
                            .push([
                                out_buf.read_sample(0, i).unwrap(),
                                out_buf.read_sample(1, i).unwrap(),
                            ])
                            .expect("worker_sender push failed");
                    }
                }
            }

            if let Some(handle) = rt_handle {
                if let Err(e) = demote_current_thread_from_real_time(handle) {
                    nice_log!("could not reset worker thread priority: {e}");
                }
            }
            nice_log!("worker thread {:?} exiting...", thread::current().id());
        });

        // wait for worker thread to fully start and for it to prefill the output buffer
        while worker_destination.is_empty() {
            if worker.is_finished() {
                let _ = worker.join();
                return None;
            }
            spin_loop();
        }

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        self.worker.replace(worker);
        self.worker_died = false;
        Some(buffer_capacity)
    }

    /// Replaces the sample with a processed one. If the worker thread has died, for example due to
    /// a panic, the sample is passed through unprocessed instead.
    pub fn process(&mut self, sample: [&mut f32; 2]) {
        let (Some(sender), Some(receiver), Some(worker)) =
            (&mut self.sender, &mut self.receiver, &self.worker)
        else {
            return;
        };
        if self.worker_died {
            return;
        }

        // a dead worker drops its ends of the queues, which shows up as them being abandoned.
        // Without checking for it these loops would wait forever and hang the host.
        if sender.is_full() {
            // worker thread is behind -> make sure it's awake and wait
            worker.thread().unpark();
            while sender.is_full() {
                if sender.is_abandoned() {
                    self.worker_died = true;
                    return;
                }
                spin_loop();
            }
        }

        sender
            .push([*sample[0], *sample[1]])
            .expect("queue was full");

        if receiver.is_empty() {
            worker.thread().unpark();
            while receiver.is_empty() {
                if receiver.is_abandoned() {
                    self.worker_died = true;
                    return;
                }
                spin_loop();
            }
        }

        let out = receiver.pop().expect("queue was empty");
        *sample[0] = out[0];
        *sample[1] = out[1];
    }

    /// Lets the worker process the samples sent so far. This should be called after each block,
    /// as the worker sleeps while it has no input.
    pub fn wake_worker(&self) {
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }

    /// Stops the worker thread if one is running and waits for it to exit
    pub fn stop(&mut self) {
        // dropping the input queue signals the worker to exit
        self.sender = None;
        self.receiver = None;
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            if worker.join().is_err() {
                nice_log!("worker thread panicked");
            }
        }
    }

    pub fn update_settings(&mut self, settings: ModelSettings) {
        self.settings.store(settings);
    }
}

impl Drop for DfWrapper {
    fn drop(&mut self) {
        self.stop();
    }
}
