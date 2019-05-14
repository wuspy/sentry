use std::fs;
use std::process;
use gstreamer::prelude::*;
use gstreamer as gst;
use tokio::prelude::*;
use futures::future;
use futures::sync::mpsc::{UnboundedSender, unbounded};
use crate::sentry::{Message, StartResult, UnboundedChannel, MessageContent, Client, MessageSource};
use crate::sentry::config::Config;
use std::net::SocketAddr;
use std::thread;
use tokio::net::UdpSocket;
use rand::prelude::*;
use std::collections::HashMap;
use crate::sentry::MessageContent::VideoError;

struct UdpHandshakeComplete {
    server_addr: SocketAddr,
    client_addr: SocketAddr,
}

struct UdpHandshake {
    socket: Option<UdpSocket>,
    local_addr: SocketAddr,
    client_addr: SocketAddr,
    buf: [u8; 32],
    nonce: String,
    out_message_sink: UnboundedSender<Message>
}

impl UdpHandshake {
    fn begin(config: &Config, client: &Client, out_message_sink: UnboundedSender<Message>) -> Result<Self, String> {
        let addr = SocketAddr::new(config.video.host.as_str().parse().unwrap(), 0);
        let socket = UdpSocket::bind(&addr)
            .map_err(|err| format!("Could not bind UDP socket for video streaming: {}", err))?;
        let local_addr = socket.local_addr()
            .map_err(|err| format!("Could not get local address for UDP socket: {}", err))?;
        let nonce: String = thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .collect();

        out_message_sink.unbounded_send(Message {
            content: MessageContent::VideoOffer {
                nonce: nonce.to_owned(),
                for_client: client.address,
                rtp_address: local_addr,
            },
            source: MessageSource::VideoServer,
        }).unwrap_or_else(|err|
            error!("Failed to send bus message: {}", err)
        );

        Ok(UdpHandshake {
            socket: Some(socket),
            local_addr,
            client_addr: client.address,
            buf: [0; 32],
            nonce,
            out_message_sink,
        })
    }
}

impl Future for UdpHandshake {
    type Item = UdpHandshakeComplete;
    type Error = String;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let (_, client_addr) = try_ready!(self.socket
                .as_mut()
                .expect("poll() called after future already completed")
                .poll_recv_from(&mut self.buf)
                .map_err(|err| format!("UDP Socket read error: {}",  err))
                );

            if client_addr.ip() != self.client_addr.ip() {
                warn!("Received UDP handshake message from wrong address (from {}, expecting {})", client_addr.ip(), self.client_addr.ip());
                continue;
            }
            if self.buf.len() == 0 {
                warn!("Received empty UDP handshake message");
                continue;
            }

            // Drop the socket before returning so the port will be available again
            drop(self.socket.take());
            let response = String::from_utf8_lossy(&self.buf).to_string();
            return if response == self.nonce {
                self.out_message_sink.unbounded_send(Message {
                    content: MessageContent::VideoStreaming {
                        for_client: self.client_addr,
                    },
                    source: MessageSource::VideoServer,
                }).unwrap_or_else(|err|
                    error!("Failed to send bus message: {}", err)
                );
                Ok(futures::Async::Ready(UdpHandshakeComplete {
                    server_addr: self.local_addr,
                    client_addr,
                }))
            } else {
                Err(format!("Received invalid nonce from {} (expected {}, got {})",  self.client_addr, self.nonce, response))
            };
        }
    }
}

fn find_camera_device(properties: &HashMap<String, String>) -> Option<String> {
    for device in fs::read_dir("/dev").ok()?
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.as_str().find("video").is_some())
        .map(|name| format!("/dev/{}", name))
    {
        let info = String::from_utf8(
            process::Command::new("udevadm")
                .arg("info")
                .arg("-a")
                .arg("-n")
                .arg(&device)
                .output()
                .ok()?
                .stdout
        ).ok()?;

        if properties
            .into_iter()
            .filter(|&(property, value)| {
                info.as_str().find(format!("ATTRS{{{}}}==\"{}\"", property, value).as_str()).is_none()
            })
            .count() == 0
        {
            // Device matches all properties
            return Some(device);
        }
    }
    None
}

pub fn start(config: Config) -> StartResult<UnboundedChannel<Message>> {
    gst::init().map_err(|err| format!("Could not initialize GStreamer: {}", err))?;
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();
    let pipeline = create_pipeline(config.clone(), out_message_sink.clone())?;

    tokio::spawn(in_message_stream
        .for_each(move |message| {
            match message.content {
                MessageContent::ClientConnected(client) => {
                    let pipeline = pipeline.clone();
                    tokio::spawn({
                        let out_message_sink = out_message_sink.clone();
                        add_client_sink(
                            pipeline,
                            config.clone(),
                            client.clone(),
                            out_message_sink.clone()
                        ).map_err(move |err| {
                            error!("Error adding video sink for {}: {}", client.address, err);
                            out_message_sink.unbounded_send(Message {
                                content: VideoError {
                                    message: err,
                                    for_client: Some(client.address),
                                },
                                source: MessageSource::VideoServer,
                            }).unwrap_or_else(|err|
                                error!("Failed to send bus message: {}", err)
                            );
                        })
                    });
                }
                MessageContent::ClientDisconnected(client) => {
                    if let Err(err) = drop_client_sink(&pipeline, &client) {
                        error!("Error dropping video sink for {}: {}", client.address, err);
                    }
                }
                _ => {}
            }
            Ok(())
        })
    );

    Ok((in_message_sink, out_message_stream))
}

fn create_pipeline(config: Config, out_message_sink: UnboundedSender<Message>) -> Result<gst::Pipeline, String> {
    info!("Creating gstreamer pipeline");
    let device = find_camera_device(&config.camera)
        .ok_or(format!("Failed to find camera device matching properties {:?}", config.camera))?;
    info!("Found camera device {}", device);
    let command = format!(
        "v4l2src device={} ! {} ! tee name=tee allow-not-linked=true",
        device,
        config.video.encoder.as_str()
    );

    info!("Creating pipeline with \"{}\"", command);
    let pipeline = gst::parse_launch(command.as_str())
        .map_err(|err| format!("Failed to parse gstreamer command \"{}\": {}", command, err))?
        .dynamic_cast::<gst::Pipeline>()
        .unwrap();

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|err| format!("Failed to set pipeline to state Playing: {}", err))?;

    let bus = pipeline.get_bus().ok_or(format!("Could not get bus for pipeline"))?;
    thread::spawn({
        let pipeline = pipeline.clone();
        move || {
            use gst::message::MessageView;
            for msg in bus.iter_timed(gst::CLOCK_TIME_NONE) {
                match msg.view() {
                    MessageView::Eos(..) => {
                        info!("Gstreamer: EOS");
                        break;
                    }
                    MessageView::Error(err) => {
                        error!("Gstreamer: Error from {}: {} {}",
                               err.get_src().map(|s| s.get_name().to_owned()).unwrap_or("?".to_owned()),
                               err.get_error(),
                               err.get_debug().unwrap_or("?".to_owned()));
                        out_message_sink.unbounded_send(Message {
                            content: VideoError {
                                message: err.get_error().to_string(),
                                for_client: None,
                            },
                            source: MessageSource::VideoServer,
                        }).unwrap_or_else(|err|
                            error!("Failed to send bus message: {}", err)
                        );
                        break;
                    }
                    MessageView::Warning(warning) => {
                        warn!("Gstreamer: Warning from {}: {} {}",
                              warning.get_src().map(|s| s.get_name().to_owned()).unwrap_or("?".to_owned()),
                              warning.get_error(),
                              warning.get_debug().unwrap_or("?".to_owned()));
                    }
                    MessageView::StateChanged(state) => {
                        if state.get_src().map(|s| s == pipeline).unwrap_or(false) {
                            info!("Gstreamer: pipeline state changed to {:?}", state.get_current());
                        } else {
                            debug!("Gstreamer: {} state changed to {:?}", state.get_src().unwrap().get_name(), state.get_current());
                        }
                    }
                    _ => {}
                };
            }

            if pipeline.set_state(gst::State::Null).is_err() {
                error!("Could not set pipeline to state Null");
            }
        }
    });

    Ok(pipeline)
}

fn get_client_queue_name(client: &Client) -> String {
    format!("queue_{}", client.address)
}

fn get_client_sink_name(client: &Client) -> String {
    format!("sink_{}", client.address)
}

fn get_client_queue(pipeline: &gst::Pipeline, client: &Client) -> Result<gst::Element, String> {
    pipeline
        .get_by_name(get_client_queue_name(client).as_str())
        .ok_or(format!("Could not find queue for client {}", client.address))
}

fn get_client_sink(pipeline: &gst::Pipeline, client: &Client) -> Result<gst::Element, String> {
    pipeline
        .get_by_name(get_client_sink_name(client).as_str())
        .ok_or(format!("Could not find sink for client {}", client.address))
}

fn get_tee(pipeline: &gst::Pipeline) -> Result<gst::Element, String> {
    pipeline
        .get_by_name("tee")
        .ok_or(format!("Could not find element tee"))
}

fn drop_client_sink(pipeline: &gst::Pipeline, client: &Client) -> Result<(), String> {
    let queue = get_client_queue(pipeline, client)?;
    let sink = get_client_sink(pipeline, client)?;
    let tee = get_tee(pipeline)?;
    tee.unlink(&queue);
    queue.unlink(&sink);
    pipeline.remove(&queue).map_err(|_| format!("Could not remove {} from pipeline", queue.get_name()))?;
    pipeline.remove(&sink).map_err(|_| format!("Could not remove {} from pipeline", sink.get_name()))?;
    queue
        .set_state(gst::State::Null)
        .map_err(|_| format!("Could not set {} to state Null", queue.get_name()))?;
    sink
        .set_state(gst::State::Null)
        .map_err(|_| format!("Could not set {} to state Null", sink.get_name()))?;

    Ok(())
}

fn add_client_sink(
    pipeline: gst::Pipeline,
    config: Config,
    client: Client,
    out_message_sink: UnboundedSender<Message>
) -> impl Future<Item=(), Error=String> {
    info!("Starting UDP handshake with client {}", client.address);
    match UdpHandshake::begin(&config, &client, out_message_sink.clone()) {
        Err(err) => future::Either::A(future::err(err)),
        Ok(handshake) => future::Either::B(handshake
            .and_then(move |handshake: UdpHandshakeComplete| {
                info!("Adding gstreamer sink for client {}", client.address);
                let queue = gst::ElementFactory::make("queue", get_client_queue_name(&client).as_str())
                    .ok_or(format!("Could not create queue element"))?;
                let sink = gst::ElementFactory::make("udpsink", get_client_sink_name(&client).as_str())
                    .ok_or(format!("Could not create udpsink element"))?;
                let tee = get_tee(&pipeline)?;

                sink.set_property_from_str("async", "false");
                sink.set_property_from_str("bind-address", format!("{}", handshake.server_addr.ip()).as_str());
                sink.set_property_from_str("bind-port", format!("{}", handshake.server_addr.port()).as_str());
                sink.set_property_from_str("host", format!("{}", handshake.client_addr.ip()).as_str());
                sink.set_property_from_str("port", format!("{}", handshake.client_addr.port()).as_str());

                pipeline.add(&queue)
                    .map_err(|_| format!("Could not add {} to pipeline", queue.get_name()))?;
                pipeline.add(&sink)
                    .map_err(|_| format!("Could not add {} to pipeline", sink.get_name()))?;
                tee
                    .link(&queue)
                    .map_err(|_| format!("Could not link {} to {}", tee.get_name(), queue.get_name()))?;
                queue
                    .link(&sink)
                    .map_err(|_| format!("Could not link {} to {}", queue.get_name(), sink.get_name()))?;
                queue
                    .set_state(gst::State::Playing)
                    .map_err(|_| format!("Could not set {} to state Playing", queue.get_name()))?;
                sink
                    .set_state(gst::State::Playing)
                    .map_err(|_| format!("Could not set {} to state Playing", sink.get_name()))?;

                Ok(())
            })
            .map_err(|err| format!("{}", err))
        )
    }
}
