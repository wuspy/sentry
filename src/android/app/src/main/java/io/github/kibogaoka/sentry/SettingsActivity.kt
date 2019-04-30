package io.github.kibogaoka.sentry

import android.app.Activity
import android.content.Intent
import android.content.SharedPreferences
import android.os.Bundle
import android.preference.PreferenceManager
import android.text.Editable
import android.text.TextWatcher
import android.util.Log
import android.view.WindowManager

import kotlinx.android.synthetic.main.activity_settings.*
import java.lang.Exception
import java.net.InetSocketAddress
import java.net.Socket
import java.net.URISyntaxException

class SettingsActivity : Activity() {
    companion object {
        val DEFAULT_SERVER_ADDRESS = InetSocketAddress("10.42.0.1", 8080)
    }

    private lateinit var serverAddress: InetSocketAddress
    private lateinit var preferences: SharedPreferences

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(null)
        setContentView(R.layout.activity_settings)

        preferences = PreferenceManager.getDefaultSharedPreferences(this)
        serverAddress = try {
            parseSocketAddress(preferences.getString("server_host", "")!!)
        } catch (e: Exception) {
            DEFAULT_SERVER_ADDRESS
        }

        server_address_input.setText("${serverAddress.hostString}:${serverAddress.port}")
        server_address_input.addTextChangedListener(object: TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}

            override fun afterTextChanged(s: Editable?) {
                testAddress(s.toString())
            }

        })

        back_button.setOnClickListener { onBackPressed() }

        ok_button.setOnClickListener {
            with (preferences.edit()) {
                putString("server_host", server_address_input.text.toString())
                commit()
            }
            onBackPressed()
        }

        testAddress(server_address_input.text.toString())
    }

    override fun onBackPressed() {
        finish()
        startActivity(Intent(this, MainActivity::class.java))
    }

    private fun testAddress(address: String) {
        Thread(Runnable {
            fun update(text: String, color: Int, image: Int) {
                runOnUiThread {
                    if (address == server_address_input.text.toString()) {
                        connection_status_text.text = text
                        connection_status_image.setImageResource(image)
                        connection_status_text.setTextColor(getColor(resources, color))
                    }
                }
            }

            update("Connecting", R.color.yellow, R.drawable.ic_more_horiz_yellow_24dp)
            try {
                val socket = Socket()
                val parsedAddress = parseSocketAddress(address)
                socket.connect(parsedAddress, 1000)
                update("Connected", R.color.green, R.drawable.ic_cloud_done_green_24dp)
                socket.close()
            } catch (e: URISyntaxException) {
                update("Invalid address", R.color.red, R.drawable.ic_cloud_off_red_24dp)
            } catch (e: Exception) {
                update("Connection failed", R.color.red, R.drawable.ic_cloud_off_red_24dp)
            }
        }).start()
    }
}
