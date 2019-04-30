package io.github.kibogaoka.sentry

import android.content.res.Resources
import android.os.Build
import java.net.InetSocketAddress
import java.net.URI
import java.net.URISyntaxException

fun parseSocketAddress(address: String): InetSocketAddress {
    val uri = URI("my://$address")
    if (uri.host == null || uri.port == -1) {
        throw URISyntaxException(uri.toString(), "Address must contain a host and port")
    }

    return InetSocketAddress(uri.host, uri.port)
}

fun getColor(resources: Resources, color: Int): Int {
    return if (Build.VERSION.SDK_INT < 23) {
        resources.getColor(color)
    } else {
        resources.getColor(color, null)
    }
}
