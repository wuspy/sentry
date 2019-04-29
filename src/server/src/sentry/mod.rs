extern crate serde_json;
extern crate glib;
extern crate gstreamer;
extern crate rand;
extern crate tokio;
extern crate tokio_serial;
extern crate tokio_fs;
extern crate tokio_io;
extern crate bytes;
extern crate byteorder;

use std::net::SocketAddr;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver};
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub enum Command {
    Move {
        pitch: f64,
        yaw: f64,
    },
    Fire,
    OpenBreach,
    CloseBreach,
    CycleBreach,
    FireAndCycleBreach,
    Home,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum HardwareStatus {
    Ready,
    Homing,
    Firing,
    Error,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Client {
    pub address: SocketAddr,
    pub queue_position: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageSource {
    Arduino,
    WebsocketServer,
    VideoServer,
    Client (Client),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageContent {
    HardwareState {
        pitch_pos: u32,
        yaw_pos: u32,
        status: HardwareStatus,
    },
    VideoOffer {
        nonce: String,
        for_client: SocketAddr,
        rtp_address: SocketAddr,
    },
    VideoStreaming {
        for_client: SocketAddr,
    },
    Command (Command),
    ClientConnected (Client),
    ClientDisconnected (Client),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub content: MessageContent,
    pub source: MessageSource,
}

pub type StartResult<T> = Result<T, String>;
pub type UnboundedChannel<T> = (UnboundedSender<T>, UnboundedReceiver<T>);

pub mod server;
pub mod arduino;
pub mod video;
pub mod config;