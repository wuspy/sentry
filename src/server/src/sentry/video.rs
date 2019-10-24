use crate::sentry::config::Config;
use crate::sentry::MessageContent::VideoError;
use crate::sentry::{Bus, BusSender, Client, Message, MessageContent, MessageSource};
use futures::future;
use futures::oneshot;
use gstreamer as gst;
use gstreamer::prelude::*;
use rand::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::process;
use std::thread;
use tokio::net::UdpSocket;
use tokio::prelude::*;

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
    bus_sink: BusSender<Message>,
}

impl UdpHandshake {
    fn begin(
        config: Config,
        client: Client,
        bus_sink: BusSender<Message>,
    ) -> impl Future<Item = UdpHandshakeComplete, Error = String> {
        let addr = SocketAddr::new(config.video.host.as_str().parse().unwrap(), 0);
        let nonce: String = thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(32)
            .collect();

        future::result(UdpSocket::bind(&addr))
            .map_err(|err| format!("Could not bind UDP socket for video streaming: {}", err))
            .and_then(|socket| {
                let local_addr = socket.local_addr().map_err(|err| {
                    format!("Could not get local address for UDP socket: {}", err)
                })?;
                Ok((socket, local_addr))
            })
            .and_then(move |(socket, local_addr)| {
                // Send a message on the bus indicating the video handshake for this client is about to begin
                bus_sink
                    .unbounded_send(Message {
                        content: MessageContent::VideoOffer {
                            nonce: nonce.to_owned(),
                            for_client: client.address,
                            rtp_address: local_addr,
                        },
                        source: MessageSource::VideoServer,
                    })
                    .unwrap();

                UdpHandshake {
                    socket: Some(socket),
                    local_addr,
                    client_addr: client.address,
                    buf: [0; 32],
                    nonce,
                    bus_sink,
                }
            })
    }
}

impl Future for UdpHandshake {
    type Item = UdpHandshakeComplete;
    type Error = String;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let (_, client_addr) = try_ready!(self
                .socket
                .as_mut()
                .expect("poll() called after future already completed")
                .poll_recv_from(&mut self.buf)
                .map_err(|err| format!("UDP Socket read error: {}", err)));

            if client_addr.ip() != self.client_addr.ip() {
                warn!(
                    "Received UDP handshake message from wrong address (from {}, expecting {})",
                    client_addr.ip(),
                    self.client_addr.ip()
                );
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
                self.bus_sink
                    .unbounded_send(Message {
                        content: MessageContent::VideoStreaming {
                            for_client: self.client_addr,
                        },
                        source: MessageSource::VideoServer,
                    })
                    .unwrap_or_else(|err| error!("Failed to send bus message: {}", err));
                Ok(futures::Async::Ready(UdpHandshakeComplete {
                    server_addr: self.local_addr,
                    client_addr,
                }))
            } else {
                Err(format!(
                    "Received invalid nonce from {} (expected {}, got {})",
                    self.client_addr, self.nonce, response
                ))
            };
        }
    }
}

fn find_camera_device(properties: &HashMap<String, String>) -> Option<String> {
    for device in fs::read_dir("/dev")
        .ok()?
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
                .stdout,
        )
        .ok()?;

        if properties
            .into_iter()
            .filter(|&(property, value)| {
                info.as_str()
                    .find(format!("ATTRS{{{}}}==\"{}\"", property, value).as_str())
                    .is_none()
            })
            .count()
            == 0
        {
            // Device matches all properties
            return Some(device);
        }
    }
    None
}

pub fn start(config: Config, bus: Bus<Message>) -> impl Future<Item = (), Error = String> {
    let (bus_sink, bus_stream) = bus;

    future::result(gst::init().map_err(|err| format!("Could not initialize GStreamer: {}", err)))
        .and_then({
            let config = config.clone();
            move |_| create_pipeline(config)
        })
        .and_then(move |pipeline| {
            bus_stream
                .for_each({
                    let pipeline = pipeline.clone();
                    let config = config.clone();
                    move |message| {
                        match message.content {
                            MessageContent::ClientConnected(client) => {
                                tokio::spawn(
                                    add_client_sink(
                                        pipeline.clone(),
                                        config.clone(),
                                        client.clone(),
                                        bus_sink.clone(),
                                    )
                                    .or_else({
                                        let bus_sink = bus_sink.clone();
                                        move |err| {
                                            error!(
                                                "Error adding video sink for {}: {}",
                                                client.address, err
                                            );
                                            bus_sink
                                                .unbounded_send(Message {
                                                    content: VideoError {
                                                        message: err,
                                                        for_client: Some(client.address),
                                                    },
                                                    source: MessageSource::VideoServer,
                                                })
                                                .unwrap();
                                            Ok(())
                                        }
                                    }),
                                );
                            }
                            MessageContent::ClientDisconnected(client) => {
                                if let Err(err) = drop_client_sink(&pipeline, &client) {
                                    error!(
                                        "Error dropping video sink for {}: {}",
                                        client.address, err
                                    );
                                }
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                })
                .map(|_| ())
                .map_err(|err| format!("Error in bus receiver loop: {:?}", err))
                .select(play_pipeline_future(pipeline))
                .map_err(|(err, _)| err)
                .map(|_| ())
        })
}

fn create_pipeline(config: Config) -> Result<gst::Pipeline, String> {
    info!("Creating gstreamer pipeline");
    let device = find_camera_device(&config.camera).ok_or(format!(
        "Failed to find camera device matching properties {:?}",
        config.camera
    ))?;
    info!("Found camera device {}", device);
    let command = format!(
        "videotestsrc pattern=smpte ! {} ! tee name=tee allow-not-linked=true",
        //device,
        config.video.encoder.as_str()
    );

    info!("Creating pipeline with \"{}\"", command);
    let pipeline = gst::parse_launch(command.as_str())
        .map_err(|err| format!("Failed to parse gstreamer command \"{}\": {}", command, err))?
        .dynamic_cast::<gst::Pipeline>()
        .unwrap();

    Ok(pipeline)
}

fn play_pipeline_future(pipeline: gst::Pipeline) -> impl Future<Item = (), Error = String> {
    let (tx, rx) = oneshot::<Result<(), String>>();
    thread::spawn({
        let pipeline = pipeline.clone();
        move || tx.send(play_pipeline(pipeline))
    });

    rx.map_err(|err| format!("Error communicating with thread: {}", err))
        .and_then(|result| result)
}

fn play_pipeline(pipeline: gst::Pipeline) -> Result<(), String> {
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|err| format!("Failed to set pipeline to state Playing: {}", err))?;

    use gst::message::MessageView;
    let bus = pipeline
        .get_bus()
        .ok_or(format!("Could not get bus for pipeline"))?;
    for msg in bus.iter_timed(gst::CLOCK_TIME_NONE) {
        match msg.view() {
            MessageView::Eos(..) => {
                // Should never happen, since this is a live stream from a camera
                stop_pipeline(&pipeline).unwrap();
                return Err(format!("EOS"));
            }
            MessageView::Error(err) => {
                stop_pipeline(&pipeline).unwrap();
                return Err(format!(
                    "Error from {}: {} {}",
                    err.get_src()
                        .map(|s| s.get_name().to_owned())
                        .unwrap_or("?".to_owned()),
                    err.get_error(),
                    err.get_debug().unwrap_or("?".to_owned())
                ));
            }
            MessageView::Warning(warning) => {
                warn!(
                    "Gstreamer: Warning from {}: {} {}",
                    warning
                        .get_src()
                        .map(|s| s.get_name().to_owned())
                        .unwrap_or("?".to_owned()),
                    warning.get_error(),
                    warning.get_debug().unwrap_or("?".to_owned())
                );
            }
            MessageView::StateChanged(state) => {
                if state.get_src().map(|s| s == pipeline).unwrap_or(false) {
                    info!(
                        "Gstreamer: pipeline state changed to {:?}",
                        state.get_current()
                    );
                } else {
                    debug!(
                        "Gstreamer: {} state changed to {:?}",
                        state.get_src().unwrap().get_name(),
                        state.get_current()
                    );
                }
            }
            _ => {}
        };
    }

    Ok(())
}

fn stop_pipeline(pipeline: &gst::Pipeline) -> Result<(), String> {
    pipeline
        .set_state(gst::State::Null)
        .map(|_| ())
        .map_err(|err| format!("Failed to set pipeline to state Null: {}", err))
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
        .ok_or(format!(
            "Could not find queue for client {}",
            client.address
        ))
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
    pipeline
        .remove(&queue)
        .map_err(|_| format!("Could not remove {} from pipeline", queue.get_name()))?;
    pipeline
        .remove(&sink)
        .map_err(|_| format!("Could not remove {} from pipeline", sink.get_name()))?;
    queue
        .set_state(gst::State::Null)
        .map_err(|_| format!("Could not set {} to state Null", queue.get_name()))?;
    sink.set_state(gst::State::Null)
        .map_err(|_| format!("Could not set {} to state Null", sink.get_name()))?;

    Ok(())
}

fn add_client_sink(
    pipeline: gst::Pipeline,
    config: Config,
    client: Client,
    bus_sink: BusSender<Message>,
) -> impl Future<Item = (), Error = String> {
    info!("Starting UDP handshake with client {}", client.address);
    UdpHandshake::begin(config.clone(), client.clone(), bus_sink.clone()).and_then(
        move |handshake| {
            info!("Adding gstreamer sink for client {}", client.address);
            let queue = gst::ElementFactory::make("queue", get_client_queue_name(&client).as_str())
                .ok_or(format!("Could not create queue element"))?;
            let sink = gst::ElementFactory::make("udpsink", get_client_sink_name(&client).as_str())
                .ok_or(format!("Could not create udpsink element"))?;
            let tee = get_tee(&pipeline)?;

            sink.set_property_from_str("async", "false");
            sink.set_property_from_str(
                "bind-address",
                format!("{}", handshake.server_addr.ip()).as_str(),
            );
            sink.set_property_from_str(
                "bind-port",
                format!("{}", handshake.server_addr.port()).as_str(),
            );
            sink.set_property_from_str("host", format!("{}", handshake.client_addr.ip()).as_str());
            sink.set_property_from_str(
                "port",
                format!("{}", handshake.client_addr.port()).as_str(),
            );

            pipeline
                .add(&queue)
                .map_err(|_| format!("Could not add {} to pipeline", queue.get_name()))?;
            pipeline
                .add(&sink)
                .map_err(|_| format!("Could not add {} to pipeline", sink.get_name()))?;
            tee.link(&queue).map_err(|_| {
                format!("Could not link {} to {}", tee.get_name(), queue.get_name())
            })?;
            queue.link(&sink).map_err(|_| {
                format!("Could not link {} to {}", queue.get_name(), sink.get_name())
            })?;
            queue
                .set_state(gst::State::Playing)
                .map_err(|_| format!("Could not set {} to state Playing", queue.get_name()))?;
            sink.set_state(gst::State::Playing)
                .map_err(|_| format!("Could not set {} to state Playing", sink.get_name()))?;

            Ok(())
        },
    )
}
