extern crate byteorder;
extern crate bytes;
extern crate glib;
extern crate gstreamer;
extern crate rand;
extern crate serde_json;
extern crate tokio;
extern crate tokio_fs;
extern crate tokio_io;
extern crate tokio_serial;

use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub enum Command {
    Move { pitch: f64, yaw: f64 },
    Home,
    Fire,
    ReleaseMagazine,
    LoadMagazine,
    Reload,
    FireAndReload,
    MotorsOn,
    MotorsOff,
}

#[derive(Clone)]
pub enum HardwareStatus {
    Ready,
    NotLoaded,
    MagazineReleased,
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
    Client(Client),
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
    VideoError {
        message: String,
        for_client: Option<SocketAddr>,
    },
    Command(Command),
    ClientConnected(Client),
    ClientDisconnected(Client),
    Ping,
}

#[derive(Clone)]
pub struct Message {
    pub content: MessageContent,
    pub source: MessageSource,
}

pub mod bus;
pub use bus::*;

pub mod arduino;
pub mod config;
pub mod server;
pub mod video;
