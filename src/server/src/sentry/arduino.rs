use std::time::{Duration, SystemTime};
use std::mem;
use std::io;
use byteorder::{BigEndian, LittleEndian, ByteOrder};
use tokio::codec::{Decoder, Encoder};
use tokio::reactor::Handle;
use bytes::{BytesMut, BufMut};
use tokio_serial::{Serial, SerialPort, SerialPortSettings, Parity, DataBits, StopBits, FlowControl};
use futures::{Stream, Sink};
use tokio::prelude::*;
use crate::sentry::{Command, HardwareStatus, Message, MessageContent, MessageSource, StartResult, UnboundedChannel};
use futures::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use crc::crc16::checksum_usb as crc16;
use crate::sentry::config::Config;
use std::sync::Mutex;
use crate::sentry::MessageContent::HardwareState;

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
        if src.len() >= 11 {
            let (our_crc, their_crc) = (BigEndian::read_u16(src), crc16(&src[2..11]));
            if our_crc == their_crc {
                let message = src.split_to(11);
                Ok(Some(MessageContent::HardwareState {
                    status: match message[2] {
                        100 => HardwareStatus::Ready,
                        101 => HardwareStatus::NotLoaded,
                        102 => HardwareStatus::MagazineReleased,
                        103 => HardwareStatus::Reloading,
                        104 => HardwareStatus::HomingRequired,
                        105 => HardwareStatus::Homing,
                        106 => HardwareStatus::MotorsOff,
                        107 => HardwareStatus::HomingFailed,
                        _ => HardwareStatus::Error,
                    },
                    pitch_pos: BigEndian::read_u32(&message[3..]),
                    yaw_pos: BigEndian::read_u32(&message[7..]),
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
            Command::Move {..}              => 200,
            Command::Home                   => 201,
            Command::ReleaseMagazine        => 202,
            Command::LoadMagazine           => 203,
            Command::Reload                 => 204,
            Command::Fire                   => 205,
            Command::FireAndReload          => 206,
            Command::MotorsOn               => 207,
            Command::MotorsOff              => 208,
        };
        match item {
            Command::Move { pitch, yaw } => {
                BigEndian::write_i32(&mut message[3..], (pitch * self.config.arduino.pitch_max_speed as f64) as i32);
                BigEndian::write_i32(&mut message[7..], (yaw * self.config.arduino.yaw_max_speed as f64) as i32);
            }
            Command::Home => {
                BigEndian::write_u32(&mut message[3..], self.config.arduino.pitch_homing_speed);
                BigEndian::write_u32(&mut message[7..], self.config.arduino.yaw_homing_speed);
            }
            _ => {}
        };
        let crc = crc16(&message[2..]);
        BigEndian::write_u16(&mut message[0..], crc);
        dst.put_slice(&message);
        Ok(())
    }
}

pub fn start(config: Config, handle: &Handle) -> StartResult<UnboundedChannel<Message>> {
    let serial_settings = SerialPortSettings {
        baud_rate: config.arduino.baud,
        parity: Parity::None,
        data_bits: DataBits::Eight,
        stop_bits: StopBits::One,
        flow_control: FlowControl::None,
        timeout: Duration::from_millis(10),
    };

    let arduino = Serial::from_path_with_handle(&config.arduino.device, &serial_settings, handle)
        .map_err(|err| format!("Cannot open {}: {}", &config.arduino.device, err))?;

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
    let mut message_count = 0;
    let mut last_calculation_time = SystemTime::now();
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
        .filter(move |command| {
            // Rate-limit the amount of commands we get to prevent straining the serial connection
            message_count += 1;
            if message_count >= 10 {
                // Allow <=10 messages/100ms
                match last_calculation_time.elapsed() {
                    Ok(duration) if duration.as_millis() < 100 => {
                        warn!("Discarding arduino command due to rate-limiting");
                        return false;
                    }
                    _ => {}
                }

                message_count = 0;
                last_calculation_time = SystemTime::now();
            }
            true
        })
        .forward(arduino_sink.sink_map_err(|_| ()))
        .and_then(|_| Ok(()))
    );

    Ok((in_message_sink, out_message_stream))
}
