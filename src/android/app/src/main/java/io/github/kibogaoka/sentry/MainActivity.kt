package io.github.kibogaoka.sentry

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Message
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.View
import kotlinx.android.synthetic.main.activity_main.*
import org.json.JSONObject
import java.io.*
import java.lang.Exception
import java.net.*

class MainActivity : Activity() {

    companion object {
        init {
            System.loadLibrary("sentry_video")
        }
    }

    private external fun getGstreamerVersion(): String
    private external fun setVideoSurface(surface: Surface)
    private external fun playVideo(command: String): String
    private external fun stopVideo()
    private external fun initVideo(): String

    private lateinit var networkThread: Thread
    private lateinit var tx: PrintWriter
    private val holePuncher = UdpHolePuncher()
    private var isPausing = false
    private var connected = false
    private var queuePosition = 0
    private var videoError = ""

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
    }

    override fun onResume() {
        super.onResume()

        initVideo()
        updateUi()

        video_surface.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) { }
            override fun surfaceDestroyed(holder: SurfaceHolder) { }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                setVideoSurface(holder.surface)
            }
        })

        fire_button.setOnClickListener {
            Log.i("Main", "Fire clicked")
            tx.println("""{"command":"fire"}""")
        }

        val handler = Handler {

                Log.i("Main", "Got ${it.data.getString("type")!!} message")
                when (it.data.getString("type")!!) {
                    "connected" -> {
                        this.connected = true
                        updateUi()
                    }
                    "disconnected" -> {
                        this.connected = false
                        stopVideo()
                        this.videoError = ""
                        updateUi()
                    }
                    "queue_position" -> {
                        this.queuePosition = it.data.getInt("queue_position")
                        this.updateUi()
                    }
                    "video_offer" -> {
                        val address = parseSocketAddress(it.data.getString("rtp_address")!!)
                        val message = it.data.getString("nonce")!!
                        holePuncher.start(address, message)
                    }
                    "video_streaming" -> {
                        holePuncher.stop()
                        this.videoError = playVideo("""
                            udpsrc port=${holePuncher.boundPort!!} !
                            ${it.data.getString("gstreamer_command")!!}
                        """)
                        updateUi()
                    }
                }

            false
        }

        isPausing = false
        networkThread = Thread(Runnable {
            fun createMessage(): Message {
                val message = Message()
                message.data = Bundle()
                return message
            }

            while (!isPausing) {
                var message: Message
                val socket = Socket()
                try {
                    socket.connect(InetSocketAddress("192.168.10.101", 8080), 1000)
                    socket.soTimeout = 100
                } catch (e: Exception) {
                    Log.w("Main", "Socket failed to connect: ${e.message}")
                    Thread.sleep(100)
                    continue
                }

                message = createMessage()
                message.data.putString("type", "connected")
                handler.sendMessage(message)

                val rx = BufferedReader(InputStreamReader(socket.getInputStream()))
                tx = PrintWriter(socket.getOutputStream())
                socket_loop@while (!socket.isClosed) {
                    if (isPausing) {
                        Log.i("Main", "Closing socket due to pausing")
                        socket.close()
                        return@Runnable
                    }

                    val line = try {
                        when (val line = rx.readLine()) {
                            null -> {
                                socket.close()
                                Log.w("Main", "Received null from socket")
                                continue@socket_loop
                            }
                            else -> line
                        }
                    } catch (e: IOException) {
                        continue@socket_loop
                    }

                    try {
                        val json = JSONObject(line)
                        message = createMessage()

                        when {
                            json.has("video_offer") -> {
                                val json = json.getJSONObject("video_offer")
                                message.data.putString("type", "video_offer")
                                message.data.putString("nonce", json.getString("nonce"))
                                message.data.putString("rtp_address", json.getString("rtp_address"))
                            }
                            json.has("video_streaming") -> {
                                val command = json
                                    .getJSONObject("video_streaming")
                                    .getString("gstreamer_command")
                                message.data.putString("type", "video_streaming")
                                message.data.putString("gstreamer_command", command)
                            }
                            json.has("queue_position") -> {
                                val pos = json.getInt("queue_position")
                                message.data.putString("type", "queue_position")
                                message.data.putInt("queue_position", pos)
                            }
                            else -> {
                                Log.w("Main", "Can't handle message $line")
                                continue@socket_loop
                            }
                        }

                        handler.sendMessage(message)
                    } catch (e: Exception) {
                        Log.w("Main", "Error reading JSON message: $line")
                    }
                }
                message = createMessage()
                message.data.putString("type", "disconnected")
                handler.sendMessage(message)
                Thread.sleep(100)
            }
        })
        networkThread.start()
    }

    override fun onPause() {
        stopVideo()
        isPausing = true
        networkThread.join()
        super.onPause()
    }

    private fun updateUi() {
        val msg = when {
            !connected -> "Connecting..."
            videoError != "" -> "Error playing stream: $videoError"
            queuePosition > 0 -> "Someone else is already in control"
            else -> null
        }
        when (msg) {
            null -> {
                message.text = ""
                message.visibility = View.GONE
            }
            else -> {
                message.text = msg
                message.visibility = View.VISIBLE
            }
        }

        if (connected && queuePosition == 0) {
            joystick.visibility = View.VISIBLE
            fire_button.visibility = View.VISIBLE
        } else {
            joystick.visibility = View.GONE
            fire_button.visibility = View.GONE
        }
    }

    private fun parseSocketAddress(address: String): InetSocketAddress {
        val uri = URI("my://$address")
        if (uri.host == null || uri.port == -1) {
            throw URISyntaxException(uri.toString(), "Address must contain a host and port")
        }

        return InetSocketAddress(uri.host, uri.port)
    }
}
