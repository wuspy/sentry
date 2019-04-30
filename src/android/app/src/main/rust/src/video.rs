use glib;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use gstreamer_video::prelude::*;
use std::net::SocketAddr;
use std::thread;
use std::thread::JoinHandle;

// These are needed for the compiler to statically link the needed plugins into this library
#[link(name="gstcoreelements")]
extern "C" { fn gst_plugin_coreelements_register(); }
#[link(name="gstvideotestsrc")]
extern "C" { fn gst_plugin_videotestsrc_register(); }
#[link(name="gstvideoconvert")]
extern "C" { fn gst_plugin_videoconvert_register(); }
#[link(name="gstaudioconvert")]
extern "C" { fn gst_plugin_audioconvert_register(); }
#[link(name="gstvideorate")]
extern "C" { fn gst_plugin_videorate_register(); }
#[link(name="gstvideoscale")]
extern "C" { fn gst_plugin_videoscale_register(); }
#[link(name="gstudp")]
extern "C" { fn gst_plugin_udp_register(); }
#[link(name="gstrtp")]
extern "C" { fn gst_plugin_rtp_register(); }
#[link(name="gstrtpmanager")]
extern "C" { fn gst_plugin_rtpmanager_register(); }
#[link(name="gstandroidmedia")]
extern "C" { fn gst_plugin_androidmedia_register(); }
#[link(name="gstjpeg")]
extern "C" { fn gst_plugin_jpeg_register(); }
#[link(name="gstlibav")]
extern "C" { fn gst_plugin_libav_register(); }
#[link(name="gstplayback")]
extern "C" { fn gst_plugin_playback_register(); }
#[link(name="gstmpegtsdemux")]
extern "C" { fn gst_plugin_mpegtsdemux_register(); }
#[link(name="gstaudioparsers")]
extern "C" { fn gst_plugin_audioparsers_register(); }
#[link(name="gstopengl")]
extern "C" { fn gst_plugin_opengl_register(); }

fn register_gstreamer_plugins() {
    unsafe {
        gst_plugin_coreelements_register();
        gst_plugin_videotestsrc_register();
        gst_plugin_videoconvert_register();
        gst_plugin_audioconvert_register();
        gst_plugin_videorate_register();
        gst_plugin_videoscale_register();
        gst_plugin_udp_register();
        gst_plugin_rtp_register();
        gst_plugin_rtpmanager_register();
        gst_plugin_androidmedia_register();
        gst_plugin_jpeg_register();
        gst_plugin_libav_register();
        gst_plugin_playback_register();
        gst_plugin_mpegtsdemux_register();
        gst_plugin_audioparsers_register();
        gst_plugin_opengl_register();
    }
}

pub struct Video {
    thread: Option<JoinHandle<()>>,
    main_loop: Option<glib::MainLoop>,
    pipeline: Option<gst::Pipeline>,
    native_surface: usize,
    command: Option<String>,
}

impl Video {
    pub fn init() -> Result<(), String> {
        gst::init().map_err(|err| format!("Failed to initialize GStreamer: {}", err))?;
        register_gstreamer_plugins();
        Ok(())
    }

    pub fn get_gst_version() -> String {
        gst::version_string().to_string()
    }

    pub fn new(native_surface: usize) -> Self {
        Video {
            thread: None,
            main_loop: None,
            pipeline: None,
            command: None,
            native_surface,
        }
    }

    pub fn stop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            pipeline.set_state(gst::State::Null).expect("Failed to stop pipeline");
            self.command.take();
        }
        if let Some(main_loop) = self.main_loop.take() {
            main_loop.quit();
        }
        if let Some(thread) = self.thread.take() {
            thread.join();
        }
    }

    pub fn start(&mut self, command: String) -> Result<(), String> {
        self.stop();

        let main_loop = glib::MainLoop::new(None, false);
        let pipeline = gst::parse_launch(command.as_str())
            .map_err(|err| format!("Failed to parse command: {}", err))?
            .dynamic_cast::<gst::Pipeline>()
            .unwrap();

        pipeline.get_bus().unwrap().add_watch(move |_, msg| {
            use gst::message::MessageView;
            match msg.view() {
                MessageView::Error(msg) =>
                    error!("GStreamer: {}", msg.get_debug().unwrap_or("".into())),
                MessageView::Warning(msg) =>
                    warn!("GStreamer: {}", msg.get_debug().unwrap_or("".into())),
                MessageView::Info(msg) =>
                    info!("GStreamer: {}", msg.get_debug().unwrap_or("".into())),
                _ => {}
            };

            glib::Continue(true)
        });

        let sink = pipeline
            .get_by_interface(gst_video::VideoOverlay::static_type())
            .ok_or(format!("Pipeline contains no sink implementing VideoOverlay"))?
            .dynamic_cast::<gst_video::VideoOverlay>()
            .unwrap();

        unsafe { sink.set_window_handle(self.native_surface); }

        pipeline.set_state(gst::State::Playing)
            .map_err(|err| format!("Failed to play pipeline: {}", err))?;

        self.pipeline = Some(pipeline);
        self.main_loop = Some(main_loop.clone());
        self.command = Some(command);
        self.thread = Some(thread::spawn(move || main_loop.run()));
        Ok(())
    }

    pub fn get_command(&self) -> Option<String> {
        self.command.as_ref().map(|command| command.to_owned())
    }
}
