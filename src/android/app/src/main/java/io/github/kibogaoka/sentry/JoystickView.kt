package io.github.kibogaoka.sentry

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.*
import android.util.AttributeSet
import android.util.TypedValue
import android.view.MotionEvent
import android.view.View
import java.util.Timer
import kotlin.concurrent.scheduleAtFixedRate
import kotlin.math.*

class JoystickView : View {

    // Default sizes in DP
    var zoneDiameter = 200f

    var buttonDiameter = 60f

    var zoneColor = Color.parseColor("#FFFFFF")
        set(value) {
            field = value
            _zonePaint.color = value
        }

    var buttonColor = Color.parseColor("#FFFFFF")
        set(value) {
            field = value
            _buttonPaint.color = value
        }

    var interval: Long = 0
        set(value) {
            field = value
            _timer?.cancel()
            _timer = null
            if (value > 0) {
                _timer = Timer()
                _timer!!.scheduleAtFixedRate(0, interval) {
                    if (isEnabled) {
                        _listener?.invoke(location)
                    }
                }
            }
        }

    var location = PointF()
        private set

    var deadzone = 0.1F

    private var _pressed = false
    private var _ringAnimator = ValueAnimator()
    private var _origin = PointF()
    private var _rawLocation = PointF()
    private var _animatedSize = 0f
    private var _zonePaint = Paint()
    private var _buttonPaint = Paint()
    private var _timer: Timer? = null
    private var _listener: ((PointF) -> Unit)? = null

    constructor (context: Context?) : super(context) {
        init(null)
    }

    constructor (context: Context?, attributes: AttributeSet) : super(context, attributes) {
        init(attributes)
    }

    constructor(context: Context?, attributes: AttributeSet, defStyleAttr: Int)
            : super(context, attributes, defStyleAttr) {
        init(attributes)
    }

    constructor(context: Context?, attributes: AttributeSet, defStyleAttr: Int, defStyleRes: Int)
            : super(context, attributes, defStyleAttr, defStyleRes) {
        init(attributes)
    }

    fun setOnUpdateListener(listener: ((PointF) -> Unit)?) {
        _listener = listener
    }

    private fun init(attributes: AttributeSet?) {
        val metrics = resources.displayMetrics

        val ta = context.obtainStyledAttributes(attributes, R.styleable.JoystickView)

        zoneDiameter = ta.getDimension(
            R.styleable.JoystickView_zone_diameter,
            TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, zoneDiameter, metrics)
        )
        buttonDiameter = ta.getDimension(
            R.styleable.JoystickView_button_diameter,
            TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, buttonDiameter, metrics)
        )
        deadzone = min(abs(ta.getDimension(R.styleable.JoystickView_deadzone, deadzone)), 1f)

        zoneColor = ta.getColor(R.styleable.JoystickView_zone_color, zoneColor)
        buttonColor = ta.getColor(R.styleable.JoystickView_button_color, buttonColor)

        interval = ta.getInt(R.styleable.JoystickView_interval, interval.toInt()).toLong()

        _zonePaint.flags = Paint.ANTI_ALIAS_FLAG
        _buttonPaint.flags = Paint.ANTI_ALIAS_FLAG

        ta.recycle()
    }

    override fun onDraw(canvas: Canvas) {
        if (_animatedSize > 0) {
            // Draw zone
            canvas.drawCircle(_origin.x, _origin.y, _animatedSize / 2, _zonePaint)
        }
        if (_pressed) {
            // Draw button
            canvas.drawCircle(_rawLocation.x + _origin.x, _rawLocation.y + _origin.y, buttonDiameter / 2, _buttonPaint)
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.action) {
            MotionEvent.ACTION_DOWN -> {
                _pressed = true
                _ringAnimator = ValueAnimator.ofFloat(0f, zoneDiameter)
                _ringAnimator.duration = 120
                _ringAnimator.addUpdateListener {
                        animation -> _animatedSize = animation.animatedValue as Float
                    invalidate()
                }
                _ringAnimator.start()
                _origin.x = event.x
                _origin.y = event.y
            }
            MotionEvent.ACTION_UP -> {
                _pressed = false
                _ringAnimator = ValueAnimator.ofFloat(_animatedSize, 0f)
                _ringAnimator.duration = 120
                _ringAnimator.addUpdateListener {
                        animation -> _animatedSize = animation.animatedValue as Float
                    invalidate()
                }
                _ringAnimator.start()
            }
        }
        _rawLocation.x = event.x - _origin.x
        _rawLocation.y = event.y - _origin.y

        // Convert cartesian coordinates to polar and constrain the magnitude
        val angle = atan(when {
            _rawLocation.x == 0f -> when {
                _rawLocation.y > 0 -> Float.POSITIVE_INFINITY
                _rawLocation.y < 0 -> Float.NEGATIVE_INFINITY
                else -> 0f
            }
            else -> _rawLocation.y / _rawLocation.x
        })
        val zoneRadius = zoneDiameter / 2
        var magnitude = min(sqrt(_rawLocation.x * _rawLocation.x + _rawLocation.y * _rawLocation.y), zoneRadius)
        if (_rawLocation.x < 0) {
            magnitude = -magnitude
        }

        // Convert polar coordinates back to cartesian
        _rawLocation.x = cos(angle) * magnitude
        _rawLocation.y = sin(angle) * magnitude

        // Convert rawLocation to be bounded [-1, 1]
        if (_pressed) {
            location.x = _rawLocation.x / zoneRadius
            location.y = -_rawLocation.y / zoneRadius
            // Apply the dead-zone
            if (abs(location.x) < deadzone) {
                location.x = 0f
            }
            if (abs(location.y) < deadzone) {
                location.y = 0f
            }
        } else {
            location.x = 0f
            location.y = 0f
        }
        invalidate()
        return true
    }
}
