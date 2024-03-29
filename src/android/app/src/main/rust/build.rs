use std::prelude::*;
use std::env;

fn main() {
    let gstreamer_root = env::var("GSTREAMER_ROOT_ANDROID").expect("GSTREAMER_ROOT_ANDROID not specified");
    let abi = match env::var("TARGET").expect("TARGET not specified").as_str() {
        "aarch64-linux-android" => "arm64",
        "armv7-linux-androideabi" => "armv7",
        "i686-linux-android" => "x86",
        "x86_64-linux-android" => "x86_64",
        other => panic!("Target {} is not supported by GStreamer", other),
    };
    
    println!("cargo:rustc-link-search=native={}/{}/lib", gstreamer_root, abi);
    println!("cargo:rustc-link-search=native={}/{}/lib/gstreamer-1.0", gstreamer_root, abi);

    for lib in get_gstreamer_libs() {
        println!("cargo:rustc-link-lib=static={}", lib);
    }
    for lib in get_android_libs() {
        println!("cargo:rustc-link-lib=dylib={}", lib);
    }
}

fn get_android_libs() -> Vec<&'static str> {
    vec![
        "android",
        "log",
        "EGL",
    ]
}

fn get_gstreamer_libs() -> Vec<&'static str> {
    vec![
        // GStreamer plugin libs
        "gstandroidmedia",
        "gstvideoconvert",
        "gstvideorate",
        "gstvideoscale",
        "gstautoconvert",
        "gstautodetect",
        "gstplayback",
        "gstlibav",
        "gstjpeg",
        "gstopengl",
        "gstudp",
        "gstrtp",
        "gstvideotestsrc",
        "gstcoreelements",
        // GStreamer core libs & dependencies
        "gstplayer-1.0",
        "oggkate",
        "gstcheck-1.0",
        "tag",
        "FLAC",
        "avfilter",
        "dv",
        "mms",
        "turbojpeg",
        "sbc",
        "mpeg2convert",
        "kate",
        "opus",
        "gstfft-1.0",
        "vo-aacenc",
        "theoraenc",
        "graphene-1.0",
        "ass",
        "gstcodecparsers-1.0",
        "a52",
        "rtmp",
        "faad",
        "wavpack",
        "gstmpegts-1.0",
        "mp3lame",
        "x264",
        "speex",
        "srtp",
        "gstbadvideo-1.0",
        "ges-1.0",
        "vorbisenc",
        "soup-2.4",
        "charset",
        "gstwebrtc-1.0",
        "vorbisfile",
        "vorbis",
        "gstphotography-1.0",
        "gstriff-1.0",
        "vorbisidec",
        "opencore-amrnb",
        "gstcontroller-1.0",
        "dca",
        "mpg123",
        "visual-0.4",
        "gstisoff-1.0",
        "spandsp",
        "mpeg2",
        "gstbasecamerabinsrc-1.0",
        "orc-test-0.4",
        "avformat",
        "avcodec",
        "cairo-gobject",
        "gstgl-1.0",
        "cairo-script-interpreter",
        "gstallocators-1.0",
        "theoradec",
        "ssl",
        "crypto",
        "nice",
        "gnutls",
        "hogweed",
        "gmp",
        "tasn1",
        "nettle",
        "swresample",
        "avutil",
        "SoundTouch",
        "openjp2",
        "gstadaptivedemux-1.0",
        "gsturidownloader-1.0",
        "openh264",
        "gstbadaudio-1.0",
        "gstinsertbin-1.0",
        "webrtc_audio_processing",
        "gnustl",
        "gstvalidate-1.0",
        "gstpbutils-1.0",
        "gstvideo-1.0",
        "gstaudio-1.0",
        "gsttag-1.0",
        "json-glib-1.0",
        "gstrtspserver-1.0",
        "gstapp-1.0",
        "gstsdp-1.0",
        "gstrtp-1.0",
        "gstnet-1.0",
        "gstbase-1.0",
        "gstrtsp-1.0",
        "gstreamer-1.0",
        "theora",
        "ogg",
        "rsvg-2",
        "pangocairo-1.0",
        "pangoft2-1.0",
        "pango-1.0",
        "cairo",
        "pixman-1",
        "fontconfig",
        "expat",
        "harfbuzz",
        "freetype",
        "bz2",
        "croco-0.6",
        "xml2",
        "gthread-2.0",
        "gdk_pixbuf-2.0",
        "png16",
        "gio-2.0",
        "gmodule-2.0",
        "gobject-2.0",
        "ffi",
        "fribidi",
        "glib-2.0",
        "intl",
        "iconv",
        "orc-0.4",
        "opencore-amrwb",
        "tiff",
        "z",
        "jpeg",
    ]
}
