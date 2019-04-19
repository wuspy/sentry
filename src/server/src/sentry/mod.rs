extern crate serde_json;
extern crate glib;
extern crate gstreamer;
extern crate gstreamer_sdp;
extern crate gstreamer_webrtc;
extern crate tokio;
extern crate tokio_serial;
extern crate tokio_fs;
extern crate tokio_io;
extern crate bytes;
extern crate byteorder;
extern crate hyper;

use std::net::SocketAddr;
use futures::sync::mpsc::{UnboundedSender, UnboundedReceiver};

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
    Command (Command),
    WebRtcOffer {
        offer: gstreamer_webrtc::WebRTCSessionDescription,
        for_client: SocketAddr,
    },
    WebRtcAnswer {
        answer: String,
    },
    ServerIceCandidate {
        candidate: String,
        sdp_mline_index: u32,
        for_client: SocketAddr,
    },
    ClientIceCandidate {
        candidate: String,
        sdp_mline_index: u32,
    },
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

pub mod http_server;
pub mod websocket_server;
pub mod stun_server;
pub mod arduino;
pub mod video;
pub mod config;