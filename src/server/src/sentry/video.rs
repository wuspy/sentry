extern crate glib;
extern crate gstreamer as gst;
extern crate gstreamer_sdp as gst_sdp;
extern crate gstreamer_webrtc as gst_webrtc;

use gst::prelude::*;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use crate::sentry::Message;
use gstreamer::State;

const STUN_SERVER: &str = "stun://stun.l.google.com:19302";

pub fn start(device: &str) {
    info!("Creating Gstreamer pipeline...");
    let pipeline = gst::Pipeline::new("pipeline");
    let bin = gst::parse_bin_from_description(
        format!( // TODO make this more configurable
            "v4l2src device={} ! \
            image/jpeg ! \
            jpegdec ! \
            videoconvert ! \
            video/x-raw,height=720 ! \
            x264enc tune=zerolatency bitrate=5000 ! \
            video/x-h264,profile=main ! \
            rtph264pay config-interval=3 pt=96 ! \
            webrtcbin name='webrtc' ",
            device
        ).as_str(),
        true
    ).unwrap();

    let webrtc = bin.get_by_name("webrtc").unwrap();
    webrtc.set_property_from_str("stun-server", STUN_SERVER);
    webrtc.set_property_from_str("bundle-policy", "max-bundle");
    webrtc.connect("on-negotiation-needed", false, move |values| {
        // TODO
        None
    }).unwrap();

    webrtc.connect("on-ice-candidate", false, move |values| {
        // TODO
        None
    }).unwrap();

    let bus = pipeline.get_bus().unwrap();
    bus.add_watch(move |_, msg| {
        use gst::message::MessageView;

        match msg.view() {
            MessageView::Error(err) => {
                error!("Gstreamer error: {}", err.get_debug().unwrap());
                panic!(err.get_debug().unwrap());
            },
            MessageView::Warning(warning) => {
                warn!("Gstreamer: {}", warning.get_debug().unwrap());
            }
            _ => {}
        };

        glib::Continue(true)
    });

    pipeline.add(&bin).unwrap();
    pipeline.set_state(State::Playing).unwrap();
    info!("Gstreamer pipeline created");
}
