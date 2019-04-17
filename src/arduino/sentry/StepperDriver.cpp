#include "StepperDriver.h"

using namespace Sentry;

StepperDriver::StepperDriver(uint8_t stepPin, uint8_t dirPin, uint8_t enablePin, bool invertDir)
{
    _enablePin = enablePin;
    _invertDir = invertDir;

    pinMode(stepPin, OUTPUT);
    pinMode(dirPin, OUTPUT);
    pinMode(enablePin, OUTPUT);

    _stepBitmask = digitalPinToBitMask(stepPin);
    _stepRegister = portOutputRegister(digitalPinToPort(stepPin));

    _dirBitmask = digitalPinToBitMask(dirPin);
    _dirRegister = portOutputRegister(digitalPinToPort(dirPin));

    _speed = 0.0;
    _stepInterval = 0;
    _lastStepTime = 0;
    _lastCalculationTime = 0;
    _targetPosition = _position = 0;

    setMaxSpeed(100);
    setRecalculationInterval(10000);
    setAcceleration(10);
    setDirection(DIRECTION_CW);
    setEnabled(true);
}

void StepperDriver::setEnabled(bool enabled)
{
    digitalWrite(_enablePin, enabled ? LOW : HIGH);
    _enabled = enabled;
}

void StepperDriver::setAcceleration(float acceleration)
{
    if (acceleration == 0.0) {
        return;
    }
    _acceleration = abs(acceleration);
}

void StepperDriver::setPosition(int64_t position)
{
    _position = position;
}

void StepperDriver::setMaxSpeed(float speed)
{
    _maxSpeed = abs(speed);
}

void StepperDriver::moveTo(int64_t position)
{
    _targetPosition = position;
}

int64_t StepperDriver::getDistanceToGo()
{
    return _targetPosition - _position;
}

void StepperDriver::poll()
{
    if (!_enabled) {
        return;
    }

    uint64_t timestamp = micros();
    uint64_t timeSinceCalculation = timeDiff(timestamp, _lastCalculationTime);
    if (timeSinceCalculation >= _calculationInterval) {
        computeNewSpeed(timeSinceCalculation);
        _lastCalculationTime = timestamp;
    }
    if (_stepInterval > 0 && timeDiff(timestamp, _lastStepTime) >= _stepInterval / 2) {
        if (!toggleStep()) {
            _position += static_cast<int64_t>(_direction);
        }
        _lastStepTime = timestamp;
    }
}

void StepperDriver::wait()
{
    if (!_enabled) {
        return;
    }
    while (getDistanceToGo() != 0) {
        poll();
    }
}

void StepperDriver::emergencyStop()
{
    _targetPosition = _position;
    _speed = 0.0;

    if (*_stepRegister & _stepBitmask) {
        toggleStep();
    }
}

void StepperDriver::computeNewSpeed(uint64_t elapsedTime)
{
    // Steps to reach target position
    int64_t stepsToGo = getDistanceToGo();
    // Steps to slow down to a stop from current speed
    int64_t stepsToStop = (int64_t)((_speed * _speed) / (2.0 * _acceleration));
    // Maximum allowable change in speed for this calculation
    float delta = elapsedTime / 1000000.0 * _acceleration;

    if (stepsToGo == 0 && stepsToStop <= 1) {
        // Stopped at the target position
        _speed = 0;
    } else if (_speed > _maxSpeed) {
        // Going too fast and need to slow down
        _speed = max(_maxSpeed, _speed - delta);
    } else if ((stepsToGo >= 0) ^ (static_cast<int64_t>(_direction) < 0)) {
        // Moving in the right direction
        if (stepsToStop >= abs(stepsToGo)) {
            // Getting close to target position and need to slow down
            _speed = max(0, _speed - delta);
        } else {
            // Far enough away from target position that we can continue to speed up
            _speed = min(_maxSpeed, _speed + delta);
        }
    } else if (delta > _speed) {
        // Moving in wrong direction, but going slow enough to change directions
        setDirection(_direction == DIRECTION_CW ? DIRECTION_CCW : DIRECTION_CW);
        _speed = min(_maxSpeed, delta - _speed);
    } else {
        // Moving in wrong direction, need to slow down
        _speed -= delta;
    }

    _stepInterval = _speed > 0 ? ceil(1.0 / _speed * 1000000.0) : 0;
}

uint64_t StepperDriver::timeDiff(uint64_t current, uint64_t previous)
{
    return current >= previous ? current - previous : current + (UINT64_MAX - previous);
}

bool StepperDriver::toggleStep()
{
    if (*_stepRegister & _stepBitmask) {
        *_stepRegister &= ~_stepBitmask;
        return false;
    } else {
        *_stepRegister |= _stepBitmask;
        return true;
    }
}

void StepperDriver::setDirection(Direction direction)
{
    _direction = direction;
    if ((direction == DIRECTION_CW) ^ _invertDir) {
        *_dirRegister |= _dirBitmask;
    } else {
        *_dirRegister &= ~_dirBitmask;
    }
}
