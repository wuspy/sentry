use std::time::Duration;
use std::mem;
use std::io;
use byteorder::{BigEndian, LittleEndian, ByteOrder};
use tokio::codec::{Decoder, Encoder};
use tokio::reactor::Handle;
use bytes::{BytesMut, BufMut};
use tokio_serial::{Serial, SerialPortSettings, Parity, DataBits, StopBits, FlowControl};
use futures::{Stream, Sink};
use tokio::prelude::*;
use crate::sentry::{Command, HardwareStatus, Message, MessageContent, MessageSource};
use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use crc::crc16::checksum_usb as crc16;
use crate::sentry::config::Config;
use crate::sentry::MessageSource::Arduino;

struct ArduinoCodec {
    config: Config,
}

impl ArduinoCodec {
    pub fn new(config: Config) -> Self {
        ArduinoCodec {
            config,
        }
    }
}

impl Decoder for ArduinoCodec {
    type Item = MessageContent;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() >= 10 {
            let (our_crc, their_crc) = (BigEndian::read_u16(src), crc16(&src[2..10]));
            if our_crc == their_crc {
                let message = src.split_to(10);
                Ok(Some(MessageContent::HardwareState {
                    status: HardwareStatus::Ready,
                    pitch_pos: BigEndian::read_u32(&message[2..]),
                    yaw_pos: BigEndian::read_u32(&message[6..]),
                }))
            } else {
                warn!("Arduino CRC mismatch: {:#X}/{:#X}", our_crc, their_crc);
                src.advance(1);
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }
}

impl Encoder for ArduinoCodec {
    type Item = Command;
    type Error = io::Error;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.reserve(11);
        let mut message: [u8; 11] = [0; 11];
        message[2] = match item {
            Command::Fire                   => 1,
            Command::OpenBreach             => 2,
            Command::CloseBreach            => 3,
            Command::CycleBreach            => 4,
            Command::FireAndCycleBreach     => 5,
            Command::Home                   => 6,
            _ => 0,
        };
        BigEndian::write_i32(&mut message[3..], match item {
            Command::Move {pitch, ..} =>  (pitch * self.config.arduino.max_pitch_speed as f64) as i32,
            _ => 0,
        });
        BigEndian::write_i32(&mut message[7..], match item {
            Command::Move {yaw, ..} =>  (yaw * self.config.arduino.max_yaw_speed as f64) as i32,
            _ => 0,
        });
        let crc = crc16(&message[2..]);
        BigEndian::write_u16(&mut message[0..], crc);
        dst.put_slice(&message);
        Ok(())
    }
}

pub fn start(config: Config, handle: &Handle) -> (UnboundedSender<Message>, UnboundedReceiver<Message>) {
    let serial_settings = SerialPortSettings {
        baud_rate: config.arduino.baud,
        parity: Parity::None,
        data_bits: DataBits::Eight,
        stop_bits: StopBits::One,
        flow_control: FlowControl::None,
        timeout: Duration::from_millis(10),
    };

    let arduino = Serial::from_path_with_handle(&config.arduino.device, &serial_settings, handle).unwrap();
    let (arduino_sink, arduino_stream) = ArduinoCodec::new(config.clone()).framed(arduino).split();
    let (in_message_sink, in_message_stream) = unbounded::<Message>();
    let (out_message_sink, out_message_stream) = unbounded::<Message>();

    // Spawn a task to forward arduino messages to the server through an unbounded channel
    tokio::spawn(arduino_stream
        .map_err(|_| ())
        .map(|message| {
            Message {
                content: message,
                source: MessageSource::Arduino,
            }
        })
        .forward(out_message_sink.sink_map_err(|_| ()))
        .and_then(|_| Ok(()))
    );

    // Spawn a task to forward server messages to the arduino
    tokio::spawn(in_message_stream
        .map_err(|_| ())
        .filter_map(move |message| {
            match &message.source {
                // Ignore messages from clients that aren't first in the queue
                MessageSource::Client(client) if client.queue_position > 0 => None,
                _ => match message.content {
                    // Match Command messages only
                    MessageContent::Command(command) => Some(command),
                    _ => None,
                }
            }
        })
        .filter(move |message| {
            // TODO rate-limit the amount of commands we get to prevent straining the serial connection
            true
        })
        .forward(arduino_sink.sink_map_err(|_| ()))
        .and_then(|_| Ok(()))
    );

    (in_message_sink, out_message_stream)
}
