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
import android.content.Intent
import android.content.SharedPreferences
import android.os.*
import android.preference.PreferenceManager
import android.view.View.*
import android.widget.CompoundButton
import android.widget.SeekBar
import java.lang.IllegalArgumentException
import java.util.*
import kotlin.concurrent.scheduleAtFixedRate

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
    private var sentryState = SentryState.MOTORS_OFF
    private var pitchPosition = 0
    private var yawPosition = 0
    private lateinit var serverAddress: InetSocketAddress
    private var sensitivity = 100
    private var reloadAfterFiring = false
    private var invertY = false
    private var invertX = false
    private var pingTimer = Timer()
    private lateinit var preferences: SharedPreferences
    private lateinit var placeholderVideoCommand: String

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
        sensitivity = preferences.getInt("sensitivity", 100)
        reloadAfterFiring = preferences.getBoolean("reload_after_firing", true)
        invertY = preferences.getBoolean("invert_y", false)
        invertX = preferences.getBoolean("invert_x", false)
        serverAddress = try {
            parseSocketAddress(preferences.getString("server_host", "")!!)
        } catch (e: Exception) {
            ServerSettingsActivity.DEFAULT_SERVER_ADDRESS
        }

        sensitivity_slider.progress = sensitivity
        sensitivity_value.text = "$sensitivity%"
        invert_y_checkbox.isChecked = invertY
        invert_x_checkbox.isChecked = invertX
        reload_after_firing_checkbox.isChecked = reloadAfterFiring

        video_surface.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) { }
            override fun surfaceDestroyed(holder: SurfaceHolder) { }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                Log.i(logTag, "Video surface changed")
                setVideoSurface(holder.surface)
                val ppi = resources.displayMetrics.density
                val videoWidth = (width / ppi).toInt()
                val videoHeight = (height / ppi).toInt()
                placeholderVideoCommand = "videotestsrc pattern=snow ! video/x-raw,width=$videoWidth,height=$videoHeight,framerate=24/1 ! glimagesink"
                startNetwork()
            }
        })

        joystick.interval = 50 // ms
        joystick.responseCurve = JoystickView.ResponseCurve.Exponential(1.4f)
        joystick.setOnUpdateListener {
            tx?.println(String.format(
                """{"pitch":%.3f,"yaw":%.3f}""",
                it.y * (sensitivity / 100f) * if (invertY) -1 else 1,
                it.x * (sensitivity / 100f) * if (invertX) -1 else 1
            ))
        }

        fire_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand(if (reloadAfterFiring) Command.FIRE_AND_RELOAD else Command.FIRE)
        }

        home_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand(Command.HOME)
        }

        mag_release_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand(if (sentryState == SentryState.MAGAZINE_RELEASED) Command.LOAD_MAGAZINE else Command.RELEASE_MAGAZINE)
        }

        reload_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand(Command.RELOAD)
        }

        motors_button.setOnClickListener {
            vibrateButtonPress()
            sendCommand(if (sentryState == SentryState.MOTORS_OFF) Command.MOTORS_ON else Command.MOTORS_OFF)
        }

        menu_button.setOnClickListener { toggleMenu() }

        set_server_address_button.setOnClickListener {
            finish()
            startActivity(Intent(this, ServerSettingsActivity::class.java))
        }

        sensitivity_slider.setOnSeekBarChangeListener(object: SeekBar.OnSeekBarChangeListener {
            override fun onStartTrackingTouch(seekBar: SeekBar?) {}
            override fun onStopTrackingTouch(seekBar: SeekBar?) {}

            override fun onProgressChanged(seekBar: SeekBar?, progress: Int, fromUser: Boolean) {
                sensitivity = progress
                sensitivity_value.text = "$progress%"
            }
        })

        reload_after_firing_checkbox.setOnCheckedChangeListener { _: CompoundButton, b: Boolean ->
            reloadAfterFiring = b
        }

        invert_y_checkbox.setOnCheckedChangeListener { _: CompoundButton, b: Boolean ->
            invertY = b
        }

        invert_x_checkbox.setOnCheckedChangeListener { _: CompoundButton, b: Boolean ->
            invertX = b
        }

        pingTimer.scheduleAtFixedRate(0, 500) {
            tx?.println("\"ping\"")
        }

        updateUi()
    }

    override fun onStop() {
        Log.i(logTag, "Stopping")
        stopVideo()
        stopNetwork()
        with (preferences.edit()) {
            putInt("sensitivity", sensitivity)
            putBoolean("reload_after_firing", reloadAfterFiring)
            putBoolean("invert_y", invertY)
            putBoolean("invert_x", invertX)
            apply()
        }
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
                playVideo(placeholderVideoCommand)
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
                            json.has("video_error") -> {
                                holePuncher.stop()
                                videoError = json.getJSONObject("video_error").getString("message")!!
                                updateUi()
                            }
                            json.has("queue_position") -> {
                                this.queuePosition =  json.getInt("queue_position")
                                updateUi()
                            }
                            json.has("status") -> {
                                sentryState = try {
                                    SentryState.valueOf(json.getString("status")!!.toUpperCase())
                                } catch (e: IllegalArgumentException) {
                                    Log.w(logTag, "Received invalid sentry status ${json.getString("status")}")
                                    sentryState
                                }
                                pitchPosition = json.getInt("pitch")
                                yawPosition = json.getInt("yaw")
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

    private fun sendCommand(command: Command) {
        Thread(Runnable { tx?.println("""{"command":"${command.toString().toLowerCase()}"}""") }).start()
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
                            menu.visibility = INVISIBLE
                            updateUi()
                        }
                        .start()
                    settings_container.animate()
                        .alpha(0f)
                        .setDuration(200)
                        .withEndAction {
                            settings_container.visibility = INVISIBLE
                        }
                        .start()
                }
                else -> {
                    menu_button.setImageResource(R.drawable.ic_arrow_back_white_24dp)
                    menu.visibility = VISIBLE
                    settings_container.alpha = 0f
                    settings_container.visibility = VISIBLE
                    updateUi()
                    menu.animate()
                        .translationX(0f)
                        .setDuration(200)
                        .start()
                    settings_container.animate()
                        .alpha(1f)
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
                sentryState == SentryState.ERROR -> "Hardware error, restart Arduino"
                sentryState == SentryState.HOMING_FAILED -> "Homing failed"
                sentryState == SentryState.HOMING_REQUIRED -> "Homing required"
                sentryState == SentryState.HOMING -> "Homing..."
                sentryState == SentryState.MOTORS_OFF -> "Motors are turned off"
                sentryState == SentryState.NOT_LOADED -> "Reload"
                sentryState == SentryState.RELOADING -> "Reloading..."
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
            val menuOpen = menu.visibility == VISIBLE
            val canMove = isActiveClient && when (sentryState) {
                SentryState.READY, SentryState.NOT_LOADED, SentryState.MAGAZINE_RELEASED -> true
                else -> false
            }

            joystick.visibility = if (isActiveClient && !menuOpen && canMove) VISIBLE else GONE
            joystick.isEnabled = joystick.visibility == View.VISIBLE
            fire_button.visibility = if (isActiveClient && !menuOpen && sentryState == SentryState.READY) VISIBLE else GONE
            home_button.visibility = if (isActiveClient && sentryState != SentryState.MOTORS_OFF) VISIBLE else GONE
            mag_release_button.visibility = if (isActiveClient && (sentryState == SentryState.NOT_LOADED || sentryState == SentryState.MAGAZINE_RELEASED)) VISIBLE else GONE
            reload_button.visibility = if (isActiveClient && sentryState == SentryState.NOT_LOADED) VISIBLE else GONE
            motors_button.visibility = if (isActiveClient) VISIBLE else GONE

            motors_button.text = if (sentryState == SentryState.MOTORS_OFF) "Turn Motors On" else "Turn Motors Off"
            mag_release_button.text = if (sentryState == SentryState.MAGAZINE_RELEASED) "Load Magazine" else "Magazine Release"
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
