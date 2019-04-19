use std::prelude::*;
use gstreamer::prelude::*;
use gstreamer as gst;
use gstreamer_sdp as gst_sdp;
use gstreamer_webrtc as gst_webrtc;
use tokio::prelude::*;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use crate::sentry::{Message, StartResult, UnboundedChannel, MessageSource, MessageContent};
use gstreamer::State;
use crate::sentry::config::Config;
use std::net::SocketAddr;
use std::collections::HashMap;

pub fn start(config: Config) -> StartResult<UnboundedChannel<Message>> {
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();

    let pipeline = gst::Pipeline::new("pipeline");
    let bin = gst::parse_bin_from_description(config.video.encoder.as_str(), true)
        .map_err(|err| format!("Could not parse GStreamer encoder: {}", err))?;
    let bus = pipeline.get_bus().ok_or(format!("Could not get bus for pipeline"))?;

    bus.add_watch(move |_, msg| {
        use gst::message::MessageView;

        match msg.view() {
            MessageView::Error(err) => {
                error!("Gstreamer: {}", err.get_debug().unwrap());
                panic!(err.get_debug().unwrap());
            },
            MessageView::Warning(warning) => {
                warn!("Gstreamer: {}", warning.get_debug().unwrap());
            }
            _ => {}
        };

        glib::Continue(true)
    });

    let tee = gst::ElementFactory::make("tee", "tee")
        .ok_or(format!("Could not create tee element"))?;

    pipeline.add(&bin).map_err(|_| format!("Could not add encoder to pipeline"))?;
    pipeline.add(&tee).map_err(|_| format!("Could not add tee to pipeline"))?;
    bin.link(&tee).map_err(|_| format!("Could not link encoder to tee"))?;

    pipeline.set_state(State::Playing)
        .map_err(|err| format!("Could not set pipeline playing: {}", err))?;

    info!("GStreamer pipeline playing");

    tokio::spawn(in_message_stream
        .map_err(|_| ())
        .for_each(move |message| {
            match &message.content {
                MessageContent::ClientConnected(client) => {
                    info!("Creating webrtc sink for client {}", client.address);
                    add_webrtc_sink(
                        client.address,
                        pipeline.clone(),
                        config.clone(),
                        out_message_sink.clone()
                    );
                },
                _ => {},
            }
            Ok(())
        })
    );

    Ok((in_message_sink, out_message_stream))
}

fn add_webrtc_sink(
    client: SocketAddr,
    pipeline: gst::Pipeline,
    config: Config,
    out_message_sink: UnboundedSender<Message>
) -> Result<(), String> {
    let queue = gst::ElementFactory::make("queue", format!("queue_{}", client).as_str())
        .ok_or(format!("Could not create queue element"))?;
    let webrtc = gst::ElementFactory::make("webrtcbin", format!("webrtc_{}", client).as_str())
        .ok_or(format!("Could not create webrtc element"))?;
    let out_message_sink_clone = out_message_sink.clone();

    webrtc.set_property_from_str("stun-server", format!("{}:{}", &config.video.stun_host, config.video.stun_port).as_str());
    webrtc.set_property_from_str("bundle-policy", "max-bundle");

    webrtc.connect("on-negotiation-needed", false, move |values| {
        on_negotiation_needed(values, client, out_message_sink.clone());
        None
    }).map_err(|_| format!("Could not connect on-negotiation-needed signal"))?;

    webrtc.connect("on-ice-candidate", false, move |values| {
        on_ice_candidate(values, client, out_message_sink_clone.clone());
        None
    }).map_err(|_| format!("Could not connect on-ice-candidate signal"))?;

    pipeline.add_many(&[
        &queue,
        &webrtc,
    ]).map_err(|_| format!("Could not add webrtc element to pipeline"))?;

    gst::Element::link_many(&[
        &pipeline.get_by_name("tee").unwrap(),
        &queue,
        &webrtc,
    ]).map_err(|_| format!("Failed to link webrtc to pipeline"))?;

    Ok(())
}

fn on_negotiation_needed(
    values: &[glib::Value],
    client: SocketAddr,
    out_message_sink: UnboundedSender<Message>
) {
    let webrtc = values[0].get::<gst::Element>().unwrap();
    let webrtc_clone = webrtc.clone();
    let promise = gst::Promise::new_with_change_func(move |promise| {
        let offer = promise
            .get_reply()
            .unwrap()
            .get_value("offer")
            .unwrap()
            .get::<gst_webrtc::WebRTCSessionDescription>()
            .unwrap();

        webrtc.emit("set-local-description", &[&offer, &None::<gst::Promise>]);

        out_message_sink.unbounded_send(Message {
            content: MessageContent::WebRtcOffer {
                for_client: client,
                offer: offer.clone(),
            },
            source: MessageSource::VideoServer,
        });
    });

    webrtc_clone.emit("create-offer", &[&None::<gst::Structure>, &promise]);
}

fn on_ice_candidate(
    values: &[glib::Value],
    client: SocketAddr,
    out_message_sink: UnboundedSender<Message>
) {
    let sdp_mline_index = values[1].get::<u32>().unwrap();
    let candidate = values[2].get::<String>().unwrap();

    out_message_sink.unbounded_send(Message {
        content: MessageContent::ServerIceCandidate {
            for_client: client,
            candidate,
            sdp_mline_index,
        },
        source: MessageSource::VideoServer,
    });
}
