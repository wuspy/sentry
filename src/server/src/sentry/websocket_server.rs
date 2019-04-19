use tokio::reactor::Handle;
use websocket::server::r#async::Server;
use websocket::client::r#async::{TcpStream, ClientNew};
use websocket::message::OwnedMessage;
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use serde_json::json;
use std::sync::{RwLock, Arc};
use crate::sentry::{Command, Message, Client, MessageContent, MessageSource, StartResult, UnboundedChannel};
use tokio::prelude::*;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded};
use crate::sentry::config::Config;

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
            client.tx.unbounded_send(message).unwrap();
        } else {
            error!("Attempted to send message to invalid client address {}", client);
        }
    }

    pub fn send_client_states(&mut self) {
        let len = self.clients.len();
        for i in 0..len {
            self.clients[i].tx
                .unbounded_send(json!({
                    "queue_position": i,
                    "num_clients": len,
                    "stun_server": format!("{}:{}", &self.config.video.stun_host.as_str(), self.config.video.stun_port),
                })
                .to_string())
                .unwrap();
        }
    }
}

pub fn start(config: Config, handle: &Handle) -> StartResult<UnboundedChannel<Message>> {
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();
    let clients = Arc::new(RwLock::new(ClientQueue::new(config.clone())));
    let clients2 = clients.clone();
    let addr = SocketAddr::new("127.0.0.1".parse().unwrap(), config.websocket.port);

    info!("Binding websocket server on {}...", addr);
    let mut server = Server::bind(&addr, handle)
        .map_err(|err| format!("Could not bind websocket server: {}", err))?;

    info!("Websocket server running");

    tokio::spawn(server
        .incoming()
        .map_err(|err| {
            error!("Client connection error: {}", err.error);
        })
        .for_each( move |(upgrade, addr)| {
            info!("Client {} has connected", addr);
            handle_client(upgrade.accept(), config.clone(), addr, out_message_sink.clone(), clients.clone());
            Ok(())
        })
    );

    tokio::spawn(in_message_stream
        .map_err(|_| ())
        .for_each(move |message| {
            let clients = clients2.clone();
            match message.content {
                MessageContent::ServerIceCandidate {for_client, candidate, sdp_mline_index} => {
                    // TODO
                },
                MessageContent::WebRtcOffer {for_client, offer } => {
                    // TODO
                },
                _ => {}
            }
            Ok(())
        })
    );

    Ok((in_message_sink, out_message_stream))
}

fn handle_client(
    client: ClientNew<TcpStream>,
    config: Config,
    addr: SocketAddr,
    out_message_sink: UnboundedSender<Message>,
    clients: Arc<RwLock<ClientQueue>>
) {
    tokio::spawn(client
        .map_err(|err| {
            error!("Client connection error: {}", err);
        })
        .and_then(move |(client, _)| {
            let (client_sink, client_stream) = client.split();
            let (tx, rx) = unbounded::<String>();
            let queue_position = clients.write().unwrap().enqueue(addr, tx);

            // Spawn a task to handle writing data from rx to this client's sink
            tokio::spawn(rx
                .map_err(|_| ())
                .filter_map(|msg| Some(OwnedMessage::Text(msg)))
                .forward(client_sink.sink_map_err(|_| ()))
                .and_then(|_| Ok(()))
            );

            out_message_sink
                .sink_map_err(|_| ())
                // Send a message notifying the server that a client has connected
                .send(Message {
                    content: MessageContent::ClientConnected(Client {
                        address: addr,
                        queue_position,
                    }),
                    source: MessageSource::WebsocketServer,
                })
                // Process the client's incoming messages
                .and_then(move |app_sink| {
                    client_stream
                        .map_err(|_| ())
                        .filter_map(move |message: OwnedMessage| {
                            match message {
                                OwnedMessage::Text(text) => match process_message(text) {
                                    Some(content) => {
                                        Some(Message {
                                            content,
                                            source: MessageSource::Client(Client {
                                                address: addr,
                                                queue_position: clients.read().unwrap()
                                                    .index_of(addr)
                                                    .unwrap_or(std::usize::MAX),
                                            })
                                        })
                                    },
                                    None => {
                                        warn!("Received invalid message from {}", addr);
                                        None
                                    },
                                },
                                OwnedMessage::Close(_) => {
                                    info!("Client {} has disconnected", addr);
                                    let mut clients = clients.write().unwrap();
                                    let queue_position = clients
                                        .index_of(addr)
                                        .unwrap_or(std::usize::MAX);
                                    clients.remove(addr);
                                    Some(Message {
                                        content: MessageContent::ClientDisconnected(Client {
                                            address: addr,
                                            queue_position,
                                        }),
                                        source: MessageSource::WebsocketServer,
                                    })
                                },
                                _ => None
                            }
                        })
                        .forward(app_sink)
                        .and_then(|_| Ok(()))
                })
        })
    );
}

fn process_message(message: String) -> Option<MessageContent> {
    use serde_json::Value::{String as JsonString, Number as JsonNumber};

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(message.as_str()) {
        if let JsonString(command) = &json["command"] {
            match command.as_str() {
                "fire" => Some(MessageContent::Command(Command::Fire)),
                "open_breach" => Some(MessageContent::Command(Command::OpenBreach)),
                "close_breach" => Some(MessageContent::Command(Command::CloseBreach)),
                "cycle_breach" => Some(MessageContent::Command(Command::CycleBreach)),
                "fire_and_cycle_breach" => Some(MessageContent::Command(Command::FireAndCycleBreach)),
                "home" => Some(MessageContent::Command(Command::Home)),
                other => {
                    warn!("Received invalid command '{}' from client", command);
                    None
                }
            }
        } else if let (JsonNumber(pitch), JsonNumber(yaw)) = (&json["pitch"], &json["yaw"]) {
            Some(MessageContent::Command(Command::Move {
                pitch: pitch.as_f64().unwrap_or(0.0),
                yaw: yaw.as_f64().unwrap_or(0.0),
            }))
        // TODO other message types
        } else {
            None
        }
    } else {
        None
    }
}
