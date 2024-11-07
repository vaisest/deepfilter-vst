use df::tract::*;
use ndarray::Array2;
use nih_plug::nih_log;
use rtrb::RingBuffer;
use rubato::{FftFixedIn, FftFixedOut, Resampler};
use std::{
    hint::spin_loop,
    thread,
    time::{Duration, Instant},
};

type Sample = [f32; 2];

/// Wrap a `DfTract` instance behind a worker thread.
/// Note: this will add latency to the input, but this
/// is required due to the model not implementing Send
pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

struct IOResampler {
    input: FftFixedOut<f32>,
    output: FftFixedIn<f32>,
}

impl DfWrapper {
    pub fn new() -> Self {
        Self {
            sender: None,
            receiver: None,
            worker: None,
        }
    }

    /// initialises model in worker thread and attaches input and output buffers to it
    pub fn init(&mut self, plugin_sample_rate: usize) {
        // 16s
        let buffer_capacity = (480 * 2).max(480);

        // create two ring buffers: one for receiving samples from plugin, and another for sending them back
        // plugin_sender -> worker_input -> **worker processing** -> worker_sender -> worker_destination
        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(buffer_capacity);
        let (mut worker_sender, worker_destination) = RingBuffer::<Sample>::new(buffer_capacity);

        let worker = thread::spawn(move || {
            let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
                .expect("initialising df failed");

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
                "worker thread {:?} starting with model sr: {}",
                thread::current().id(),
                model.sr,
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
                    continue;
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

                    // replace noisy with model_in_buf
                    for c in 0..2 {
                        for i in 0..model.hop_size {
                            noisy[[c, i]] = model_in_buf[c][i];
                        }
                    }

                    model
                        .process(noisy.view(), enhanced.view_mut())
                        .expect("model processing failed");

                    // replace model_out_buf with enhanced
                    for c in 0..2 {
                        for i in 0..model.hop_size {
                            model_out_buf[c][i] = enhanced[[c, i]];
                        }
                    }

                    // resample output
                    resampler
                        .output
                        .process_into_buffer(&model_out_buf, &mut out_buf, None)
                        .expect("error while resampling output");

                    for i in 0..model.hop_size {
                        // should never error as the same amount of samples was taken as input
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
        }

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        self.worker.replace(worker);
    }

    pub fn process(&mut self, sample: [&mut f32; 2]) {
        while self.sender.as_mut().unwrap().is_full() {
            // worker thread is busy -> wait
            spin_loop();
        }

        self.send_sample(&[*sample[0], *sample[1]]);

        // TODO: variable channel count
        let mut start = Instant::now();
        while self.receiver.as_mut().unwrap().is_empty() {
            // this is a special case, mostly for offline processing
            // it is possible that we approach end of input and no longer get new samples
            // and in this case we are essentially forced to pad with zeros
            // and we just hope for it to be correct

            // TODO: dynamic sr
            // FIXME
            if start.elapsed() >= Duration::new(2, 0) {
                start = Instant::now();
                nih_log!("waiting for a long time, padding 480 zeroes");
                self.send_zeroes(480);
            }
            // // this is probably spinning empty at the end of input.
            // there would be zero samples coming in, but the buffer
            // does not contain enough to infer another batch of samples
            spin_loop();
        }

        let out = self.receive_sample();
        *sample[0] = out[0];
        *sample[1] = out[1];
    }

    fn send_zeroes(&mut self, n: usize) {
        assert!(
            self.sender.is_some(),
            "ringbuffer does not exist when trying to send zeroes"
        );
        for _ in 0..n {
            self.send_sample(&[0.0, 0.0]);
        }
    }

    fn send_sample(&mut self, s: &Sample) {
        self.sender
            .as_mut()
            .unwrap()
            .push([s[0], s[1]])
            .expect("queue was full");
    }

    fn receive_sample(&mut self) -> Sample {
        self.receiver
            .as_mut()
            .unwrap()
            .pop()
            .expect("queue was empty")
    }
}
