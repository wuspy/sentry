package io.github.kibogaoka.sentry

import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.SocketAddress

class UdpHolePuncher {
    private var stop = false
    private var thread: Thread? = null
    private lateinit var address: SocketAddress
    private lateinit var message: String
    var boundPort: Int? = null
        private set

    private val task = Runnable {
        val socket = DatagramSocket()
        val bytes = this.message.toByteArray()
        val datagram = DatagramPacket(bytes, bytes.size, this.address)

        boundPort = socket.localPort
        loop@ while (true) {
            socket.send(datagram)
            for (i in 0..100) {
                Thread.sleep(10)
                if (this.stop) break@loop
            }
        }
        socket.disconnect()
        socket.close()
    }

    fun start(address: SocketAddress, message: String) {
        stop()
        this.address = address
        this.message = message
        this.stop = false
        this.thread = Thread(this.task)
        this.thread!!.start()

    }

    fun stop() {
        this.stop = true
        if (this.thread != null) {
            this.thread!!.join()
            this.thread = null
        }
    }
}
