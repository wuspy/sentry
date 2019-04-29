#![cfg(target_os="android")]
#![allow(non_snake_case)]

#[macro_use]
extern crate log;
extern crate android_logger;
extern crate gstreamer;
extern crate gstreamer_video;

mod video;

use std::ffi::{CString, CStr};
use jni::JNIEnv;
use jni::objects::{JObject, JString};
use jni::sys::{jstring, jobject};
use video::Video;

static mut VIDEO: Option<Video> = None;

#[link(name="android")]
extern {
    fn ANativeWindow_fromSurface(env: *mut jni::sys::JNIEnv, surface: jobject) -> usize;
}

#[no_mangle]
pub unsafe extern fn Java_io_github_kibogaoka_sentry_MainActivity_initVideo(
    env: JNIEnv,
    _: JObject
) -> jstring {
    let log_config = android_logger::Config::default()
        .with_min_level(log::Level::Info);
    android_logger::init_once(log_config);
    info!("Initializing GStreamer");
    let error = Video::init().err().unwrap_or("".into());
    info!("{}", Video::get_gst_version());
    env.new_string(error.as_str()).unwrap().into_inner()
}

#[no_mangle]
pub unsafe extern fn Java_io_github_kibogaoka_sentry_MainActivity_getGtreamerVersion(
    env: JNIEnv,
    _: JObject
) -> jstring {
    env.new_string(Video::get_gst_version()).unwrap().into_inner()
}

#[no_mangle]
pub unsafe extern fn Java_io_github_kibogaoka_sentry_MainActivity_setVideoSurface(
    env: JNIEnv,
    _: JObject,
    surface: JObject
) {
    info!("Setting native video surface {:p}", surface.into_inner());
    let surface = ANativeWindow_fromSurface(env.get_native_interface(), surface.into_inner());

    if let Some(mut old_video) = VIDEO.replace(Video::new(surface)) {
        if let Some(old_command) = old_video.get_command() {
            // The surface changed while we were playing the stream, so start the same command
            // on the new surface
            info!("Playing command \"{}\" on new surface", old_command);
            old_video.stop();
            VIDEO.as_mut().unwrap().start(old_command);
        }
    }
}

#[no_mangle]
pub unsafe extern fn Java_io_github_kibogaoka_sentry_MainActivity_playVideo(
    env: JNIEnv,
    _: JObject,
    command: JString
) -> jstring {
    let command = jstring_to_string(&env, command);

    let error = match VIDEO.as_mut() {
        Some(video) => {
            info!("Playing command \"{}\"", command);
            video.start(command).err().unwrap_or("".into())
        },
        None => format!("playVideo called before surface was set"),
    };

    env.new_string(error.as_str()).unwrap().into_inner()
}

#[no_mangle]
pub unsafe extern fn Java_io_github_kibogaoka_sentry_MainActivity_stopVideo(
    env: JNIEnv,
    _: JObject
) {
    if let Some(video) = VIDEO.as_mut() {
        info!("Stopping video");
        video.stop();
    }
}

unsafe fn jstring_to_string(env: &JNIEnv, str: JString) -> String {
    CString::from(
        CStr::from_ptr(
            env.get_string(str).unwrap().as_ptr()
        )
    ).to_str().unwrap().to_string()
}