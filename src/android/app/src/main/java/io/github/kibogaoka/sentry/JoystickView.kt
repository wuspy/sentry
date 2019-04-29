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

    interface OnUpdateListener {
        fun onUpdate(position: PointF)
    }

    // Default sizes in DP
    var zoneDiameter = 200f

    var buttonDiameter = 60f

    var borderWidth = 2f
        set(value) {
            field = value
            _zonePaint.strokeWidth = value
            _buttonPaint.strokeWidth = value
            _borderPaint.strokeWidth = value
        }

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

    var borderColor = Color.parseColor("#000000")
        set(value) {
            field = value
            _borderPaint.color = value
        }

    var interval: Long = 0
        set(value) {
            field = value
            _timer.cancel()
            if (value > 0) {
                _timer.scheduleAtFixedRate(0, interval) { emitPosition() }
            }
        }

    var location = PointF()
        private set

    private var _pressed = false
    private var _ringAnimator = ValueAnimator()
    private var _origin = PointF()
    private var _animatedSize = 0f
    private var _zonePaint = Paint()
    private var _borderPaint = Paint()
    private var _buttonPaint = Paint()
    private var _timer = Timer()
    private var _listener: OnUpdateListener? = null

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
        borderWidth = ta.getDimension(
            R.styleable.JoystickView_border_width,
            TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, borderWidth, metrics)
        )

        zoneColor = ta.getColor(R.styleable.JoystickView_zone_color, zoneColor)
        borderColor = ta.getColor(R.styleable.JoystickView_border_color, borderColor)
        buttonColor = ta.getColor(R.styleable.JoystickView_button_color, buttonColor)

        _zonePaint.style = Paint.Style.STROKE
        _borderPaint.style = Paint.Style.STROKE

        _zonePaint.flags = Paint.ANTI_ALIAS_FLAG
        _borderPaint.flags = Paint.ANTI_ALIAS_FLAG
        _buttonPaint.flags = Paint.ANTI_ALIAS_FLAG

        ta.recycle()
    }

    private fun emitPosition() {
        _listener?.onUpdate(location)
    }

    override fun onDraw(canvas: Canvas) {
        if (_animatedSize > 0) {
            // Draw outer ring
            canvas.drawCircle(_origin.x, _origin.y, _animatedSize / 2, _zonePaint)
            canvas.drawCircle(_origin.x, _origin.y, _animatedSize / 2 + borderWidth, _borderPaint)
            canvas.drawCircle(_origin.x, _origin.y, _animatedSize / 2 - borderWidth, _borderPaint)
        }
        if (_pressed) {
            // Draw button
            canvas.drawCircle(location.x + _origin.x, location.y + _origin.y, buttonDiameter / 2, _buttonPaint)
            canvas.drawCircle(location.x + _origin.x, location.y + _origin.y, buttonDiameter / 2, _borderPaint)
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
        location.x = event.x - _origin.x
        location.y = event.y - _origin.y

        // Convert cartesian coordinates to polar and constrain the magnitude
        val angle = atan(location.y / location.x)
        var magnitude = min(sqrt(location.x * location.x + location.y * location.y), zoneDiameter / 2)
        if (location.x < 0) {
            magnitude = -magnitude
        }

        // Convert polar coordinates back to cartesian
        location.x = cos(angle) * magnitude
        location.y = sin(angle) * magnitude

        invalidate()
        return true
    }
}
