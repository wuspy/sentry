use std::prelude::*;
use gstreamer::prelude::*;
use gstreamer as gst;
use tokio::prelude::*;
use futures::future;
use futures::sync::mpsc::{UnboundedSender, unbounded};
use crate::sentry::{Message, StartResult, UnboundedChannel, MessageContent, Client, MessageSource};
use gstreamer::{ClockTime};
use crate::sentry::config::Config;
use std::net::{SocketAddr, IpAddr};
use tokio::net::UdpSocket;
use tokio::prelude::*;
use rand::prelude::*;

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
        });

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
                });
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

pub fn start(config: Config) -> StartResult<UnboundedChannel<Message>> {
    gst::init().map_err(|err| format!("Could not initialize GStreamer: {}", err))?;
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();

    let pipeline = gst::Pipeline::new("pipeline");
    info!("Creating bin \"{}\"", config.video.encoder);
    let bin = gst::parse_bin_from_description(config.video.encoder.as_str(), true)
        .map_err(|err| format!("Could not parse GStreamer encoder: {}", err))?;
    let tee = gst::ElementFactory::find("tee")
        .unwrap()
        .create("tee")
        .unwrap();

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

    pipeline.add(&bin).map_err(|_| format!("Could not add \"{}\" to pipeline", config.video.encoder))?;
    pipeline.add(&tee).map_err(|_| format!("Could not add tee to pipeline"))?;
    bin.link(&tee).map_err(|_| format!("Could not link \"{}\" to tee", config.video.encoder))?;

    tokio::spawn(in_message_stream
        .map_err(|_| ())
        .for_each(move |message| {
            match message.content {
                MessageContent::ClientConnected(client) => {
                    tokio::spawn(
                        handle_new_client(
                            pipeline.clone(),
                            config.clone(),
                            client.clone(),
                            out_message_sink.clone()
                        ).map_err(move |err| error!("Error adding video sink for {}: {}", client.address, err))
                    );
                },
                MessageContent::ClientDisconnected(client) => {
                    handle_dropped_client(&pipeline, &client)
                        .map_err(move|err| error!("Error dropping video sink for {}: {}", client.address, err));
                },
                _ => {}
            }
            Ok(())
        })
    );

    Ok((in_message_sink, out_message_stream))
}

fn get_client_sink_name(client: &Client) -> String {
    format!("sink_{}", client.address)
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

fn handle_dropped_client(pipeline: &gst::Pipeline, client: &Client) -> Result<(), String> {
    let sink = get_client_sink(pipeline, client)?;
    get_tee(pipeline)?.unlink(&sink);
    pipeline.remove(&sink).map_err(|_| format!("Could not remove sink from pipeline"))?;
    if pipeline.iterate_elements().find(|element| element.get_name().starts_with("sink")) == None {
        // There are no more sinks, so stop the pipeline
        info!("Stopping GStreamer pipeline");
        pipeline.set_state(gst::State::Ready);
    }
    Ok(())
}

fn handle_new_client(
    pipeline: gst::Pipeline,
    config: Config,
    client: Client,
    out_message_sink: UnboundedSender<Message>
) -> impl Future<Item=(), Error=String> {
    match UdpHandshake::begin(&config, &client, out_message_sink.clone()) {
        Err(err) => future::Either::A(future::err(err)),
        Ok(handshake) => future::Either::B(handshake
            .and_then(move |handshake: UdpHandshakeComplete| {
                info!("UDP handshake succeeded for client {}", client.address);
                let sink = gst::ElementFactory::make("udpsink", get_client_sink_name(&client).as_str())
                    .ok_or(format!("Could not create udpsink element"))?;

                sink.set_property_from_str("bind-address", format!("{}", handshake.server_addr.ip()).as_str());
                sink.set_property_from_str("bind-port", format!("{}", handshake.server_addr.port()).as_str());
                sink.set_property_from_str("host", format!("{}", handshake.client_addr.ip()).as_str());
                sink.set_property_from_str("port", format!("{}", handshake.client_addr.port()).as_str());

                pipeline.add(&sink)
                    .map_err(|_| format!("Could not add sink to pipeline"))?;

                get_tee(&pipeline)?
                    .link(&sink)
                    .map_err(|_| format!("Could not link tee to sink"))?;

                // Make sure the pipeline is playing
                info!("Setting gstreamer pipeline playing");
                pipeline
                    .set_state(gst::State::Playing)
                    .map_err(|_| format!("Could not play pipeline"))?;

                Ok(())
            })
            .map_err(|err| format!("{}", err))
        )
    }
}
