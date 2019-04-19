extern crate serde_json;
extern crate glib;
extern crate gstreamer as gst;
extern crate gstreamer_sdp as gst_sdp;
extern crate gstreamer_webrtc as gst_webrtc;
extern crate tokio;
extern crate tokio_serial;
extern crate tokio_fs;
extern crate tokio_io;
extern crate bytes;
extern crate byteorder;
extern crate hyper;

use std::net::SocketAddr;

#[derive(Clone)]
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

#[derive(Clone)]
pub enum HardwareStatus {
    Ready,
    Homing,
    Firing,
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
    Server,
    Client (Client),
}

#[derive(Clone)]
pub enum MessageContent {
    HardwareState {
        pitch_pos: u32,
        yaw_pos: u32,
        status: HardwareStatus,
    },
    Command (Command),
    WebRtcOffer {
        offer: String,
    },
    WebRtcResponse {
        response: String,
        to: SocketAddr
    },
    ClientConnected (Client),
    ClientDisconnected (Client),
}

#[derive(Clone)]
pub struct Message {
    pub content: MessageContent,
    pub source: MessageSource,
}

pub mod http_server;
pub mod websocket_server;
pub mod arduino;
pub mod video;
pub mod config;