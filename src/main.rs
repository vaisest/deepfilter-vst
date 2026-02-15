use nih_plug::prelude::*;

use deepfilter_vst::Vst;

fn main() {
    let success = nih_export_standalone::<Vst>();
    if !success {
        println!("plugin errored or failed to initialise");
    }
}
