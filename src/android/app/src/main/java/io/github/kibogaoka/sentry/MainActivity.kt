package io.github.kibogaoka.sentry

import android.app.Activity
import android.content.Context
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.View
import kotlinx.android.synthetic.main.activity_main.*
import org.json.JSONObject
import java.io.*
import java.lang.Exception
import java.net.*
import android.content.Context.VIBRATOR_SERVICE
import android.content.Intent
import android.content.SharedPreferences
import android.opengl.Visibility
import android.os.*
import android.preference.PreferenceManager
import android.view.View.*

class MainActivity : Activity() {

    companion object {
        private val logTag = "sentry"

        init {
            System.loadLibrary("sentry_video")
        }
    }

    private external fun getGstreamerVersion(): String
    private external fun setVideoSurface(surface: Surface)
    private external fun playVideo(command: String): String
    private external fun stopVideo()
    private external fun initVideo(): String

    private var networkThread: Thread? = null
    private var socket: Socket? = null
    private lateinit var vibrator: Vibrator
    private var tx: PrintWriter? = null
    private val holePuncher = UdpHolePuncher()
    private var isStopped = false
    private var connected = false
    private var queuePosition = 0
    private var videoError = ""
    private var sentryState = "ready"
    private lateinit var serverAddress: InetSocketAddress
    private lateinit var preferences: SharedPreferences

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
    }

    override fun onStart() {
        Log.i(logTag, "Starting")
        super.onStart()
        initVideo()

        vibrator = getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        preferences = PreferenceManager.getDefaultSharedPreferences(this)
        serverAddress = try {
            parseSocketAddress(preferences.getString("server_host", "")!!)
        } catch (e: Exception) {
            SettingsActivity.DEFAULT_SERVER_ADDRESS
        }

        video_surface.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) { }
            override fun surfaceDestroyed(holder: SurfaceHolder) { }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                Log.i(logTag, "Video surface changed")
                setVideoSurface(holder.surface)
                startNetwork()
            }
        })

        joystick.interval = 50 // ms
        joystick.setOnUpdateListener {
            tx?.println(String.format("""{"pitch":%.3f,"yaw":%.3f}""", it.y, it.x))
        }

        fire_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand("fire")
        }

        home_button.setOnClickListener {
            vibrateButtonPress()
            toggleMenu()
            sendCommand("home")
        }

        open_breach_button.setOnClickListener {
            vibrateButtonPress()
            toggleMenu()
            sendCommand("open_breach")
        }

        close_breach_button.setOnClickListener {
            vibrateButtonPress()
            toggleMenu()
            sendCommand("close_breach")
        }

        menu_button.setOnClickListener { toggleMenu() }

        set_server_address_button.setOnClickListener {
            finish()
            startActivity(Intent(this, SettingsActivity::class.java))
        }

        updateUi()
    }

    override fun onStop() {
        Log.i(logTag, "Stopping")
        stopVideo()
        stopNetwork()
        super.onStop()
    }

    private fun startNetwork() {
        isStopped = false
        if (networkThread != null) {
            return
        }
        Log.i(logTag, "Initializing network")
        networkThread = Thread(Runnable {
            while (!isStopped) {
                socket = Socket()
                try {
                    socket!!.connect(serverAddress, 5000)
                    socket!!.soTimeout = 5000
                } catch (e: Exception) {
                    socket!!.close()
                    Log.w(logTag, "Socket failed to connect: ${e.message}")
                    Thread.sleep(500)
                    continue
                }

                this.connected = true
                runOnUiThread { updateUi() }

                val rx = BufferedReader(InputStreamReader(socket!!.getInputStream()))
                tx = PrintWriter(socket!!.getOutputStream(), true)
                socket_loop@while (!socket!!.isClosed) {
                    if (isStopped) {
                        Log.i(logTag, "Closing socket")
                        socket!!.close()
                        break@socket_loop
                    }

                    val line = try {
                        when (val line = rx.readLine()) {
                            null -> {
                                socket!!.close()
                                Log.w(logTag, "Received null from socket")
                                continue@socket_loop
                            }
                            else -> line
                        }
                    } catch (e: IOException) {
                        continue@socket_loop
                    }

                    try {
                        Log.d(logTag, "Server message: `$line`")
                        val json = JSONObject(line)
                        when {
                            json.has("video_offer") -> {
                                val offer = json.getJSONObject("video_offer")
                                stopVideo()
                                holePuncher.start(
                                    parseSocketAddress(offer.getString("rtp_address")!!),
                                    offer.getString("nonce")!!
                                )
                            }
                            json.has("video_streaming") -> {
                                holePuncher.stop()
                                val command = json
                                    .getJSONObject("video_streaming")
                                    .getString("gstreamer_command")
                                this.videoError = playVideo("""
                                    udpsrc port=${holePuncher.boundPort!!} !
                                    ${command!!}
                                """)
                                updateUi()
                            }
                            json.has("queue_position") -> {
                                this.queuePosition =  json.getInt("queue_position")
                                updateUi()
                            }
                            json.has("sentry_state") -> {
                                sentryState = json.getString("sentry_state")!!
                                updateUi()
                            }
                            else -> {
                                Log.w(logTag, "Can't handle message `$line`")
                                continue@socket_loop
                            }
                        }
                    } catch (e: Exception) {
                        Log.w(logTag, "Error reading JSON message `$line`")
                    }
                }
                // Disconnected
                Log.i(logTag, "Socket disconnected")
                tx = null
                this.connected = false
                this.videoError = ""
                socket = null
                updateUi()
            }
        })
        networkThread!!.start()
    }

    private fun stopNetwork() {
        Log.i(logTag, "Stopping network")
        isStopped = true
        socket?.close()
        networkThread?.join()
        networkThread = null
    }

    private fun sendCommand(command: String) {
        Thread(Runnable { tx?.println("""{"command":"$command"}""") }).start()
    }

    private fun toggleMenu() {
        runOnUiThread {
            when (menu.visibility) {
                VISIBLE -> {
                    menu_button.setImageResource(R.drawable.ic_menu_white_24dp)
                    menu.animate()
                        .translationX(-menu.width.toFloat() + 1)
                        .setDuration(200)
                        .withEndAction {
                            menu.visibility = View.INVISIBLE
                            updateUi()
                        }
                        .start()
                }
                else -> {
                    menu_button.setImageResource(R.drawable.ic_arrow_back_white_24dp)
                    menu.visibility = View.VISIBLE
                    updateUi()
                    menu.animate()
                        .translationX(0f)
                        .setDuration(200)
                        .start()
                }
            }

        }
    }

    private fun updateUi() {
        runOnUiThread {
            val msg = when {
                !connected -> "Connecting to ${serverAddress.hostString}:${serverAddress.port}..."
                videoError != "" -> "Error playing stream: $videoError"
                queuePosition > 0 -> "Someone else is already in control"
                sentryState == "error" -> "Hardware error, restart Arduino"
                sentryState == "homing" -> "Sentry is homing..."
                sentryState == "homing_required" -> "Homing required"
                sentryState == "busy" -> "Please wait..."
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

            val isActiveClient = connected && queuePosition == 0

            joystick.visibility = if (isActiveClient) VISIBLE else GONE
            joystick.isEnabled = joystick.visibility == View.VISIBLE
            fire_button.visibility = if (isActiveClient && menu.visibility != VISIBLE) VISIBLE else GONE
            home_button.visibility = if (isActiveClient) VISIBLE else GONE
            open_breach_button.visibility = if (isActiveClient) VISIBLE else GONE
            close_breach_button.visibility = if (isActiveClient) VISIBLE else GONE
        }
    }

    private fun vibrateButtonPress() {
        val ms = 50L
        if (Build.VERSION.SDK_INT < 26) {
            vibrator.vibrate(ms)
        } else {
            vibrator.vibrate(VibrationEffect.createOneShot(ms, 255))
        }
    }
}
