use std::net::SocketAddr;
use serde_json::json;
use std::sync::{RwLock, Arc, Mutex};
use crate::sentry::{Command, Message, Client, MessageContent, MessageSource, StartResult, UnboundedChannel, HardwareStatus};
use tokio::prelude::*;
use futures::sync::mpsc::{UnboundedSender, unbounded};
use crate::sentry::config::Config;
use tokio::net::{TcpListener, TcpStream};
use tokio::codec::{LinesCodec, Decoder};
use std::time::{SystemTime, Instant, Duration};
use std::cell::{Cell};
use tokio::timer::Interval;

struct ClientTx {
    address: SocketAddr,
    tx: UnboundedSender<String>,
}

pub struct ClientQueue {
    clients: Vec<ClientTx>,
}

impl ClientQueue {
    fn new() -> Self {
        ClientQueue {
            clients: Vec::new(),
        }
    }

    fn len(&self) -> usize {
        self.clients.len()
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
                self.send_client_states();
                return;
            }
        }
        error!("Attempted to remove client {}, but they are not in the queue", client);
    }

    pub fn index_of(&self, client: SocketAddr) -> Option<usize> {
        self.clients.iter().position(|c| c.address == client)
    }

    pub fn send(&mut self, client: SocketAddr, message: String) {
        if let Some(client) = self.clients.iter().find(|c| c.address == client) {
            if let Err(err) = client.tx.unbounded_send(message) {
                error!("Failed to send message to client {}: {}", client.address, err);
                warn!("Removing client {} from queue due to previous error", client.address);
                self.remove(client.address);
            }
        } else {
            error!("Attempted to send message to invalid client address {}", client);
        }
    }

    pub fn send_to_all(&mut self, message: String) {
        let len = self.clients.len();
        for i in 0..len {
            self.send(self.clients[i].address, message.to_owned());
        }
    }

    pub fn send_client_states(&mut self) {
        let len = self.clients.len();
        for i in 0..len {
            self.send(self.clients[i].address, json!({
                "queue_position": i,
                "num_clients": len,
            }).to_string());
        }
    }
}

pub fn start(config: Config) -> StartResult<UnboundedChannel<Message>> {
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();
    let clients = Arc::new(RwLock::new(ClientQueue::new()));
    let addr = SocketAddr::new(config.server.host.as_str().parse().unwrap(), config.server.port);

    info!("Binding TCP server on {}...", addr);
    let listener = TcpListener::bind(&addr)
        .map_err(|err| format!("Could not bind TCP server: {}", err))?;

    info!("TCP server running");

    // Listen for incoming connections
    tokio::spawn(listener
        .incoming()
        .map_err(|err| {
            error!("TCP client connection error: {}", err);
        })
        .for_each({
            let clients = clients.clone();
            move |socket| {
                info!("Incoming TCP connection from {}", socket.peer_addr().unwrap());
                tokio::spawn(
                    handle_client(socket, out_message_sink.clone(), clients.clone())
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
                MessageContent::VideoError { message, for_client } => {
                    let json = json!({
                        "video_error": {
                            "message": message,
                        }
                    }).to_string();

                    if let Some(client) = for_client {
                        clients.write().unwrap().send(client, json);
                    } else {
                        clients.write().unwrap().send_to_all(json);
                    }
                }
                MessageContent::HardwareState { pitch_pos, yaw_pos, status } => {
                    clients.write().unwrap().send_to_all(json!({
                        "status": match status {
                            HardwareStatus::Ready => "ready",
                            HardwareStatus::NotLoaded => "not_loaded",
                            HardwareStatus::MagazineReleased => "magazine_released",
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
    out_message_sink: UnboundedSender<Message>,
    clients: Arc<RwLock<ClientQueue>>
) -> impl Future<Item=(), Error=()> {
    let addr = socket.peer_addr().unwrap();
    let (client_sink, client_source) = LinesCodec::new().framed(socket).split();
    let (proxy_tx, proxy_rx) = unbounded::<String>();
    let queue_position = clients.write().unwrap().enqueue(addr, proxy_tx);
    let last_message_time = Arc::new(Mutex::new(Cell::new(SystemTime::now())));

    info!("Client {} has connected ({} total clients)", addr, clients.read().unwrap().len());

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
        .map_err({
            let clients = clients.clone();
            move |err| {
                error!("Error starting receiver for client {}: {}", addr, err);
                warn!("Removing client {} from queue due to previous error", addr);
                clients.write().unwrap().remove(addr);
            }
        })
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
        .inspect({
            let last_message_time = last_message_time.clone();
            move |_| {
                last_message_time.lock().unwrap().replace(SystemTime::now());
            }
        })
        // Forward all of this client's messages to the server channel
        .forward(out_message_sink.clone().sink_map_err(|_| ()))
        .map(|_| ())
        // Start a watchdog to check last_message_time
        .select(Interval::new(Instant::now(), Duration::from_secs(1))
            .take_while({
                let last_message_time = last_message_time.clone();
                let clients = clients.clone();
                move |_| {
                    Ok(match clients.read().unwrap().index_of(addr) {
                        Some(_) => match last_message_time.lock().unwrap().get().elapsed() {
                            Ok(duration) => if duration.as_secs() < 3 {
                                true
                            } else {
                                warn!("Dropping client {} because they have been inactive for 3 seconds", addr);
                                false
                            }
                            _ => {
                                error!("Could not get time since last message for client {}", addr);
                                true
                            }
                        }
                        None => {
                            warn!("Dropping receiver for client {} because they have been removed from the queue", addr);
                            false
                        }
                    })
                }
            })
            .fold((),|_, _| Ok(()))
            .map_err(|_| ())
        )
        .and_then({
            let clients = clients.clone();
            move |_| {
                // Send a message on the server channel notifying the client is disconnected
                let mut clients = clients.write().unwrap();
                info!("Client {} has disconnected ({} total clients)", addr, clients.len());
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
        .map(|_| ())
        .map_err(|_| ())
}

fn process_message(message: String) -> Option<MessageContent> {
    use serde_json::Value::{String as JsonString, Number as JsonNumber};

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(message.as_str()) {
        if let JsonString(command) = &json["command"] {
            match command.as_str() {
                "fire" => Some(MessageContent::Command(Command::Fire)),
                "release_magazine" => Some(MessageContent::Command(Command::ReleaseMagazine)),
                "load_magazine" => Some(MessageContent::Command(Command::LoadMagazine)),
                "reload" => Some(MessageContent::Command(Command::Reload)),
                "fire_and_reload" => Some(MessageContent::Command(Command::FireAndReload)),
                "home" => Some(MessageContent::Command(Command::Home)),
                "motors_on" => Some(MessageContent::Command(Command::MotorsOn)),
                "motors_off" => Some(MessageContent::Command(Command::MotorsOff)),
                _ => {
                    warn!("Received invalid command '{}' from client", command);
                    None
                }
            }
        } else if let (JsonNumber(pitch), JsonNumber(yaw)) = (&json["pitch"], &json["yaw"]) {
            Some(MessageContent::Command(Command::Move {
                pitch: pitch.as_f64()?,
                yaw: yaw.as_f64()?,
            }))
        } else if JsonString("ping".to_string()) == json {
            Some(MessageContent::Ping)
        } else {
            None
        }
    } else {
        None
    }
}
