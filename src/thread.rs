use df::tract::*;
use ndarray::Array2;
use nih_plug::nih_log;
use rtrb::RingBuffer;
use std::{
    hint::{self, spin_loop},
    thread,
};

type Sample = [f32; 2];

/// Wrap a `DfTrace` instance behind a worker thread. Note: this will add latency to the input.
pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
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
        nih_log!("nuked");
        self.sender = None;
        self.receiver = None;
        // technically it's hanging now but it should HOPEFULLY quit once the stream is empty
        self.worker = None;
    }

    pub fn init(&mut self, plugin_buffer_len: usize) {
        self.nuke_and_annihilate_self();

        let buffer_size = 4096.max(plugin_buffer_len);

        // create two ring buffers: one for receiving samples from plugin, and another for sending them back
        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(buffer_size);
        let (mut worker_sender, worker_destination) = RingBuffer::<Sample>::new(buffer_size);

        // Fill the initial buffer with zeroes
        nih_log!("sending {} zeroes...", buffer_size);
        for _ in 0..(buffer_size) {
            worker_sender.push([0.0; 2]).unwrap();
        }

        let worker = thread::spawn(move || {
            let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
                .expect("init df failed");

            nih_log!(
                "worker thread {:?} starting with model sr: {} and buffer size: {}",
                thread::current().id(),
                model.sr,
                buffer_size
            );

            // model uses ndarray, reads from in, writes to mutable out
            let mut noisy = Array2::<f32>::zeros((2, model.hop_size));
            let mut enhanced = noisy.clone();
            let mut idx = 0;

            // as long as the ring buffer exists, poll for new data
            while !worker_input.is_abandoned() {
                if worker_input.is_empty() {
                    hint::spin_loop();
                    continue;
                }

                // fill noisy array one sample at a time, until hop_size amount of samples
                let frame = worker_input.pop().unwrap();
                noisy[[0, idx]] = frame[0];
                noisy[[1, idx]] = frame[1];
                idx += 1;
                if idx == model.hop_size {
                    model.process(noisy.view(), enhanced.view_mut()).unwrap();

                    // todo: iterator
                    for x in 0..idx {
                        worker_sender
                            .push([enhanced[[0, x]], enhanced[[1, x]]])
                            .unwrap();
                    }
                    idx = 0;
                }
            }

            nih_log!("worker thread {:?} exiting...", thread::current().id());
        });

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        self.worker.replace(worker);
    }

    pub fn process(&mut self, sample: [&mut f32; 2]) -> Sample {
        // TODO: resampling when necessary
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
