use core::time;
use df::tract::*;
use ndarray::Array2;
use nih_plug::nih_log;
use rtrb::RingBuffer;
use std::thread;

type Sample = [f32; 2];

/// Wrap a `DfTrace` instance behind a worker thread. Note: this will add latency to the input.
pub struct DfWrapper {
    sender: Option<rtrb::Producer<Sample>>,
    receiver: Option<rtrb::Consumer<Sample>>,
    worker: Option<std::thread::JoinHandle<()>>,
    total: u64,
}

impl DfWrapper {
    pub fn new() -> Self {
        Self {
            sender: None,
            receiver: None,
            worker: None,
            total: 0,
        }
    }

    fn nuke_and_annihilate_self(&mut self) {
        nih_log!("nuked");
        self.sender = None;
        self.receiver = None;
        // technically it's hanging now but it should HOPEFULLY quit once the stream is empty
        self.worker = None;
    }

    pub fn init(&mut self, buffer_len: usize) {
        // if self.sender.is_some() {
        self.nuke_and_annihilate_self();
        // }

        let (plugin_sender, mut worker_input) = RingBuffer::<Sample>::new(32 * buffer_len);
        let (mut worker_sender, worker_destination) = RingBuffer::<Sample>::new(32 * buffer_len);

        // Fill the buffer with zeroes.
        nih_log!("sending {} zeroes...", 24 * buffer_len);
        for _ in 0..(24 * buffer_len) {
            worker_sender.push([0.0; 2]).unwrap();
        }

        let worker = thread::spawn(move || {
            let mut model = DfTract::new(DfParams::default(), &RuntimeParams::default_with_ch(2))
                .expect("init df failed");

            nih_log!("model sr: {}, buffer: {}", model.sr, buffer_len);

            // model uses ndarray, reads from in, writes to mutable out
            let mut noisy = Array2::<f32>::zeros((2, model.hop_size));
            let mut enhanced = noisy.clone();
            let mut idx = 0;
            while let Ok(frame) = worker_input.pop() {
                noisy[[0, idx]] = frame[0];
                noisy[[1, idx]] = frame[1];
                idx += 1;
                if idx == model.hop_size {
                    model.process(noisy.view(), enhanced.view_mut()).unwrap();

                    for x in 0..idx {
                        worker_sender
                            .push([enhanced[[0, x]], enhanced[[1, x]]])
                            .unwrap();
                    }
                    idx = 0;
                }
            }
            nih_log!("worker exiting succsefully");
        });

        self.sender.replace(plugin_sender);
        self.receiver.replace(worker_destination);
        self.worker.replace(worker);
    }

    pub fn process(&mut self, sample: [&mut f32; 2]) -> Sample {
        while self.receiver.as_mut().unwrap().is_empty() {
            thread::sleep(time::Duration::from_millis(1))
        }

        let out = self.receiver.as_mut().unwrap().pop().unwrap_or_else(|op| {
            nih_log!("EMPTY OH NO after total: {}, {op:?}", self.total);
            panic!()
        });
        self.total += 1;

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
