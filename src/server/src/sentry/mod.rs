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

#[derive(Clone, Debug)]
pub enum Command {
    Move {
        pitch: f64,
        yaw: f64,
    },
    Home,
    Fire,
    OpenBreach,
    CloseBreach,
    Reload,
    FireAndReload,
    MotorsOn,
    MotorsOff,
}

#[derive(Clone)]
pub enum HardwareStatus {
    Ready,
    NotLoaded,
    BreachOpen,
    Reloading,
    HomingRequired,
    Homing,
    MotorsOff,
    HomingFailed,
    Error,
}

#[derive(Clone)]
pub struct Client {
    pub address: SocketAddr,
    pub queue_position: usize,
}

#[derive(Clone)]
pub enum MessageSource {
    Arduino,
    WebsocketServer,
    VideoServer,
    Client (Client),
}

#[derive(Clone)]
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

#[derive(Clone)]
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