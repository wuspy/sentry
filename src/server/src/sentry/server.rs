use tokio::reactor::Handle;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use serde_json::json;
use std::sync::{RwLock, Arc};
use crate::sentry::{Command, Message, Client, MessageContent, MessageSource, StartResult, UnboundedChannel, HardwareStatus};
use tokio::prelude::*;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use crate::sentry::config::Config;
use tokio::net::{TcpListener, TcpStream};
use tokio::codec::{LinesCodec, Decoder};

struct ClientTx {
    address: SocketAddr,
    tx: UnboundedSender<String>,
}

pub struct ClientQueue {
    config: Config,
    clients: Vec<ClientTx>,
}

impl ClientQueue {
    fn new(config: Config) -> Self {
        ClientQueue {
            config,
            clients: Vec::new(),
        }
    }

    fn enqueue(&mut self, address: SocketAddr, tx: UnboundedSender<String>) -> usize {
        self.clients.push(ClientTx { address, tx });
        self.send_client_states();
        self.clients.len() - 1
    }

    fn remove(&mut self, client: SocketAddr) {
        for i in 0..self.clients.len() {
            if self.clients[i].address == client {
                self.clients.remove(i);
                break;
            }
        }
        self.send_client_states();
    }

    pub fn index_of(&self, client: SocketAddr) -> Option<usize> {
        self.clients.iter().position(|c| c.address == client)
    }

    pub fn send(&mut self, client: SocketAddr, message: String) {
        if let Some(client) = self.clients.iter().find(|c| c.address == client) {
            client.tx.unbounded_send(message);
        } else {
            error!("Attempted to send message to invalid client address {}", client);
        }
    }

    pub fn send_to_all(&mut self, message: String) {
        for client in self.clients.iter() {
            client.tx.unbounded_send(message.to_owned());
        }
    }

    pub fn send_client_states(&mut self) {
        let len = self.clients.len();
        for i in 0..len {
            self.clients[i].tx.unbounded_send(json!({
                "queue_position": i,
                "num_clients": len,
            }).to_string());
        }
    }
}

pub fn start(config: Config) -> StartResult<UnboundedChannel<Message>> {
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();
    let clients = Arc::new(RwLock::new(ClientQueue::new(config.clone())));
    let addr = SocketAddr::new(config.server.host.as_str().parse().unwrap(), config.server.port);

    info!("Binding TCP server on {}...", addr);
    let mut listener = TcpListener::bind(&addr)
        .map_err(|err| format!("Could not bind TCP server: {}", err))?;

    info!("TCP server running");

    // Listen for incoming connections
    tokio::spawn(listener
        .incoming()
        .map_err(|err| {
            error!("TCP client connection error: {}", err);
        })
        .for_each({
            let config = config.clone();
            let clients = clients.clone();
            move |socket| {
                info!("Incoming TCP connection from {}", socket.peer_addr().unwrap());
                tokio::spawn(
                    handle_client(socket, config.clone(), out_message_sink.clone(), clients.clone())
                );
                Ok(())
            }
        })
    );

    // Forward messages from the server channel to the appropriate clients
    tokio::spawn(in_message_stream
        .map_err(|_| ())
        .for_each(move |message| {
            match message.content {
                MessageContent::VideoOffer { nonce, for_client, rtp_address } => {
                    clients.write().unwrap().send(for_client, json!({
                        "video_offer": {
                            "nonce": nonce,
                            "rtp_address": rtp_address,
                        }
                    }).to_string());
                }
                MessageContent::VideoStreaming { for_client } => {
                    clients.write().unwrap().send(for_client, json!({
                        "video_streaming": {
                            "gstreamer_command": config.video.decoder,
                        }
                    }).to_string());
                }
                MessageContent::HardwareState { pitch_pos, yaw_pos, status } => {
                    clients.write().unwrap().send_to_all(json!({
                        "status": match status {
                            HardwareStatus::Ready => "ready",
                            HardwareStatus::NotLoaded => "not_loaded",
                            HardwareStatus::BreachOpen => "breach_open",
                            HardwareStatus::Reloading => "reloading",
                            HardwareStatus::HomingRequired => "homing_required",
                            HardwareStatus::Homing => "homing",
                            HardwareStatus::MotorsOff => "motors_off",
                            HardwareStatus::HomingFailed => "homing_failed",
                            HardwareStatus::Error => "error",
                        },
                        "pitch": pitch_pos,
                        "yaw": yaw_pos,
                    }).to_string());
                }
                _ => {}
            }
            Ok(())
        })
    );

    Ok((in_message_sink, out_message_stream))
}

fn handle_client(
    socket: TcpStream,
    config: Config,
    out_message_sink: UnboundedSender<Message>,
    clients: Arc<RwLock<ClientQueue>>
) -> impl Future<Item=(), Error=()> {
    let addr = socket.peer_addr().unwrap();
    let (client_sink, client_source) = LinesCodec::new().framed(socket).split();
    let (proxy_tx, proxy_rx) = unbounded::<String>();
    let queue_position = clients.write().unwrap().enqueue(addr, proxy_tx);

    info!("Client {} has connected", addr);

    // Send a message on the server channel notifying the client has connected
    out_message_sink.unbounded_send(Message {
        content: MessageContent::ClientConnected(Client {
            address: addr,
            queue_position,
        }),
        source: MessageSource::WebsocketServer,
    }).map_err(|err| format!("Failed to send message: {}", err)).unwrap();

    // Spawn a task to handle writing data from the proxy to this client's sink
    tokio::spawn(proxy_rx
        .map_err(|_| ())
        .forward(client_sink.sink_map_err(|_| ()))
        .and_then(|_| Ok(()))
    );

    client_source
        .map_err(|_| ())
        // Parse client messages to Message structs
        .filter_map({
            let clients = clients.clone();
            move |message| {
                match process_message(message) {
                    Some(content) => Some(Message {
                        content,
                        source: MessageSource::Client(Client {
                            address: addr,
                            queue_position: clients.read().unwrap()
                                .index_of(addr)
                                .unwrap_or(std::usize::MAX),
                        })
                    }),
                    _ => None
                }
            }
        })
        // Forward all of this client's messages to the server channel
        .forward(out_message_sink.clone().sink_map_err(|_| ()))
        .and_then({
            let clients = clients.clone();
            move |_| {
                // Send a message on the server channel notifying the client is disconnected
                info!("Client {} has disconnected", addr);
                let mut clients = clients.write().unwrap();
                let queue_position = clients
                    .index_of(addr)
                    .unwrap_or(std::usize::MAX);
                clients.remove(addr);
                out_message_sink.unbounded_send(Message {
                    content: MessageContent::ClientDisconnected(Client {
                        address: addr,
                        queue_position,
                    }),
                    source: MessageSource::WebsocketServer,
                }).map_err(|err| format!("Failed to send message: {}", err)).unwrap();
                Ok(())
            }
        })
}

fn process_message(message: String) -> Option<MessageContent> {
    use serde_json::Value::{String as JsonString, Number as JsonNumber, Object as JsonObject};

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(message.as_str()) {
        if let JsonString(command) = &json["command"] {
            match command.as_str() {
                "fire" => Some(MessageContent::Command(Command::Fire)),
                "open_breach" => Some(MessageContent::Command(Command::OpenBreach)),
                "close_breach" => Some(MessageContent::Command(Command::CloseBreach)),
                "reload" => Some(MessageContent::Command(Command::Reload)),
                "fire_and_reload" => Some(MessageContent::Command(Command::FireAndReload)),
                "home" => Some(MessageContent::Command(Command::Home)),
                "motors_on" => Some(MessageContent::Command(Command::MotorsOn)),
                "motors_off" => Some(MessageContent::Command(Command::MotorsOff)),
                other => {
                    warn!("Received invalid command '{}' from client", command);
                    None
                }
            }
        } else if let (JsonNumber(pitch), JsonNumber(yaw)) = (&json["pitch"], &json["yaw"]) {
            Some(MessageContent::Command(Command::Move {
                pitch: pitch.as_f64()?,
                yaw: yaw.as_f64()?,
            }))
        } else {
            None
        }
    } else {
        None
    }
}
