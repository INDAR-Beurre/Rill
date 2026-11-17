// Signal returns: gtk-rs speaks glib's Propagation now, so one place turns a
// yes/no into it.
use glib::Propagation;

pub fn inh(handled: bool) -> Propagation {
    if handled {
        Propagation::Stop
    } else {
        Propagation::Proceed
    }
}
