use nih_plug::prelude::*;

use deepfilter_vst::Vst;

fn main() {
    // this doesn't really seem to work. I'm unable to get wasapi to work even
    // if I add a mono input to the plugin
    nih_export_standalone::<Vst>();
}
