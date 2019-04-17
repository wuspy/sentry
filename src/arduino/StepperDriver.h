#ifndef SENTRY_ARDUINO_STEPPERDRIVER_H
#define SENTRY_ARDUINO_STEPPERDRIVER_H

#include <stdlib.h>
#include <Arduino.h>

namespace Sentry {
namespace Arduino {

class StepperDriver
{
public:
    typedef enum : int8_t {
        DIRECTION_CW = 1,
        DIRECTION_CCW = -1,
    } Direction;

    StepperDriver(uint8_t stepPin, uint8_t dirPin, uint8_t enablePin, bool invertDir = false);

    void setEnabled(bool enabled);
    bool isEnabled() { return _enabled; }

    void moveTo(int64_t position);
    void move(int64_t amount) { moveTo(_position + amount); }

    void poll();
    void wait();

    void setPosition(int64_t position);
    int64_t getPosition() { return _position; }

    void setAcceleration(float acceleration);
    float getAcceleration() { return _acceleration; }

    void setMaxSpeed(float speed);
    float getMaxSpeed() { return _maxSpeed; }

    float getSpeed() { return _speed; }
    Direction getDirection() { return _direction; }

    void setRecalculationInterval(uint64_t interval) { _calculationInterval = interval; }
    uint32_t getRecalculationInterval();

    int64_t getDistanceToGo();

    /// Stops the motor immediately
    void emergencyStop();

protected:
    /// Toggles the step output to the driver (two calls to this function produce one step).
    /// Returns true if step is currently HIGH, or false if LOW.
    bool toggleStep();

    /// Sets the direction output to the driver
    void setDirection(Direction direction);

    /// Computes a new speed and stepInteval
    void computeNewSpeed(uint64_t elapsedTime);

    /// Calculates the difference between current and previous timestamps, accounting for overflow
    uint64_t timeDiff(uint64_t current, uint64_t previous);

    /// Output register & bitmask for the step pin
    volatile uint8_t *_stepRegister;
    uint8_t _stepBitmask;

    /// Output register & bitmask for the dir pin
    volatile uint8_t *_dirRegister;
    uint8_t _dirBitmask;

    /// Pin number for the enable pin
    uint8_t _enablePin;
    bool _enabled;

    /// The current position in steps
    int64_t _position;

    /// The target position in steps
    int64_t _targetPosition;

    /// The current motor speed in steps/second
    /// Always positive
    float _speed;

    /// The maximum permitted speed in steps/second
    float _maxSpeed;

    /// The acceleration in steps/second^2
    float _acceleration;

    /// The current interval between steps in microseconds.
    /// 0 means the motor is stopped.
    uint64_t _stepInterval;

    /// The current interval between speed recalculations in microseconds.
    /// Must be non-zero.
    uint64_t _calculationInterval;

    /// The last step time in microseconds
    uint64_t _lastStepTime;

    /// Last speed calculation time in microseconds
    uint64_t _lastCalculationTime;

    /// Current direction the motor is spinning in
    Direction _direction;
    bool _invertDir;
};

} // namespace Arduino
} // namespace Sentry

#endif // SENTRY_ARDUINO_STEPPERDRIVER_H