 #!/bin/sh

export PKG_CONFIG_ALLOW_CROSS=1
export GSTREAMER_ROOT_ANDROID=~/.android/gstreamer-1.14.4

./gradlew cargoBuild
