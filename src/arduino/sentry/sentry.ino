#include <Arduino.h>
#include <TMC2130Stepper.h>

#include "serialize.h"
#include "config.h"
#include "pins.h"
#include "Command.h"
#include "Status.h"
#include "StepperDriver.h"

using namespace Sentry;

StepperDriver pitch(PITCH_STEP_PIN, PITCH_DIR_PIN, PITCH_ENABLE_PIN, PITCH_INVERTED);
StepperDriver yaw(YAW_STEP_PIN, YAW_DIR_PIN, YAW_ENABLE_PIN, YAW_INVERTED);
StepperDriver slide(SLIDE_STEP_PIN, SLIDE_DIR_PIN, SLIDE_ENABLE_PIN, SLIDE_INVERTED);

#ifdef PITCH_IS_TMC2130
TMC2130Stepper pitchSpi(PITCH_CS_PIN);
#endif

#ifdef YAW_IS_TMC2130
TMC2130Stepper yawSpi(YAW_CS_PIN);
#endif

#ifdef SLIDE_IS_TMC2130
TMC2130Stepper slideSpi(SLIDE_CS_PIN);
#endif

const uint32_t BUFFER_LENGTH = 512;
const uint32_t RX_MESSAGE_LENGTH = 11;
const uint32_t TX_MESSAGE_LENGTH = 11;
char *buffer = new char[BUFFER_LENGTH];
uint32_t bytesPending = 0;

/// Interval between positon status updates in microseconds
const uint64_t RESPONSE_INTERVAL = 10000;
/// Last time we've responded to the controller
uint64_t lastResponseTime = 0;
/// Last time we receive an instruction
uint64_t lastMessageTime = 0;

volatile uint8_t *ledRegister;
uint8_t ledBitmask;

Status status = STATUS_MOTORS_OFF;
bool loaded = true; // Safer to assume loaded when powered on
bool breachOpen = false;
bool homed = false;
bool homing = false;
bool homing_failed = false;
bool reloading = false;

inline void ledOn()
{
    *ledRegister |= ledBitmask;
}

inline void ledOff()
{
    *ledRegister &= ~ledBitmask;
}

void poll()
{
    pitch.poll();
    yaw.poll();
}

bool isEndstopHit(uint8_t pin)
{
    return digitalRead(pin) == HIGH;
}

bool tryHome(StepperDriver &driver, uint8_t endstopPin)
{
    // Move until endstop triggers
    while (driver.getDistanceToGo() != 0) {
        if (isEndstopHit(endstopPin)) {
            driver.setPosition(0);
            driver.emergencyStop();
            return true;
        }
        driver.poll();
    }
    return false;
}

bool homeAxis(StepperDriver &driver, int32_t max, int32_t speed, bool inverted, bool bidirectioal, int32_t offset, uint8_t endstopPin)
{
    if (!driver.isEnabled()) {
        return false;
    }

    driver.setMaxSpeed(static_cast<float>(speed));

    // Stop motor
    driver.emergencyStop();
    driver.setPosition(0);
    driver.moveTo(0);

    if (bidirectioal) {
        if (offset > 0) {
            driver.moveTo(-offset);
        } else if (offset < 0) {
            driver.moveTo(offset);
        } else {
            driver.moveTo(max / 2 * 1.1);
        }
        if (!tryHome(driver, endstopPin)) {
            if (offset > 0) {
                driver.moveTo(max - offset);
            } else if (offset < 0) {
                driver.moveTo(offset - max);
            } else {
                driver.moveTo(max / 2 * 1.1);
            }
            if (!tryHome(driver, endstopPin)) {
                return false;
            }
        }
    } else {
        driver.moveTo(max * (inverted ? 1.1 : -1.1));
        if (!tryHome(driver, endstopPin)) {
            return false;
        }
    }

    driver.setPosition(inverted ? max - offset : offset);
    driver.moveTo(driver.getPosition());
    return true;
}

void home(int32_t pitchSpeed, int32_t yawSpeed)
{
    if (!pitch.isEnabled() || !yaw.isEnabled()) {
        return;
    }

    homing = true;
    homing_failed = false;
    sendMessage();

    if (
        homeAxis(
            pitch,
            PITCH_MAX_STEPS,
            pitchSpeed,
            PITCH_HOME_INVERTED,
            PITCH_HOME_BIDIRECTIONAL,
            PITCH_HOME_OFFSET,
            PITCH_ENDSTOP_PIN
        ) &&
        homeAxis(
            yaw,
            YAW_MAX_STEPS,
            yawSpeed,
            YAW_HOME_INVERTED,
            YAW_HOME_BIDIRECTIONAL,
            YAW_HOME_OFFSET,
            YAW_ENDSTOP_PIN
        )
    ) {
        homed = true;
    } else {
        homing_failed = true;
    }
    homing = false;
}

void move(int32_t pitchSpeed, int32_t yawSpeed)
{
    if (!homed || !pitch.isEnabled() || !yaw.isEnabled()) {
        return;
    }
    pitch.setMaxSpeed(static_cast<float>(abs(pitchSpeed)));
    pitch.moveTo(pitchSpeed >= 0 ? PITCH_MAX_STEPS : 0);
    yaw.setMaxSpeed(static_cast<float>(abs(yawSpeed)));
    yaw.moveTo(yawSpeed >= 0 ? YAW_MAX_STEPS : 0);
}

void openBreach(bool disable)
{
    if (!loaded) {
        slide.setEnabled(true);
        slide.moveTo(SLIDE_OPEN_POS);
        slide.wait();
        if (disable) {
            slide.setEnabled(false);
        }
        breachOpen = true;
    }
}

void closeBreach()
{
    if (breachOpen) {
        slide.setEnabled(true);
        slide.moveTo(SLIDE_CLOSED_POS);
        slide.wait();
        slide.setEnabled(false);
        loaded = true;
        breachOpen = false;
    }
}

void fire(bool disable)
{
    if (loaded) {
        slide.setEnabled(true);
        slide.moveTo(SLIDE_FIRED_POS);
        slide.wait();
        slide.moveTo(0);
        slide.setPosition(0);
        if (disable) {
            slide.setEnabled(false);
        }
        status = STATUS_NOT_LOADED;
        loaded = false;
    }
}

void reload()
{
    if (!loaded) {
        reloading = true;
        sendMessage();
        openBreach(false);
        closeBreach();
        reloading = false;
    }
}

uint64_t timeDiff(uint64_t current, uint64_t previous)
{
    return current >= previous ? current - previous : current + (UINT64_MAX - previous);
}

void setup()
{
    ledBitmask = digitalPinToBitMask(LED_BUILTIN);
    ledRegister = portOutputRegister(digitalPinToPort(LED_BUILTIN));

    pinMode(PITCH_ENDSTOP_PIN, INPUT_PULLUP);
    pinMode(YAW_ENDSTOP_PIN, INPUT_PULLUP);

    pitch.setMaxSpeed(0);
    pitch.setAcceleration(PITCH_ACCEL);
    pitch.setEnabled(false);
    #ifdef PITCH_IS_TMC2130
    pitchSpi.begin();
    pitchSpi.SilentStepStick2130(PITCH_CURRENT);
    pitchSpi.hold_current(min(ceil(PITCH_HOLD_CURRENT / (float)PITCH_CURRENT * 31.0), 31));
    pitchSpi.microsteps(PITCH_MICROSTEPS);
    pitchSpi.interpolate(true);
    pitchSpi.stealthChop(PITCH_STEALTHCHOP);
    #endif

    yaw.setMaxSpeed(0);
    yaw.setAcceleration(YAW_ACCEL);
    yaw.setEnabled(false);
    #ifdef YAW_IS_TMC2130
    yawSpi.begin();
    yawSpi.SilentStepStick2130(YAW_CURRENT);
    yawSpi.hold_current(min(ceil(YAW_HOLD_CURRENT / (float)YAW_CURRENT * 31.0), 31));
    yawSpi.microsteps(YAW_MICROSTEPS);
    yawSpi.interpolate(true);
    yawSpi.stealthChop(YAW_STEALTHCHOP);
    #endif

    slide.setEnabled(false);
    slide.setAcceleration(SLIDE_ACCEL);
    slide.setMaxSpeed(SLIDE_SPEED);
    slide.setPosition(SLIDE_CLOSED_POS);
    slide.moveTo(SLIDE_CLOSED_POS);
    #ifdef SLIDE_IS_TMC2130
    slideSpi.begin();
    slideSpi.SilentStepStick2130(SLIDE_CURRENT);
    slideSpi.hold_current(min(ceil(SLIDE_HOLD_CURRENT / (float)SLIDE_CURRENT * 31.0), 31));
    slideSpi.microsteps(SLIDE_MICROSTEPS);
    slideSpi.interpolate(true);
    slideSpi.stealthChop(SLIDE_STEALTHCHOP);
    #endif

    ledOn();
    Serial.begin(115200);
    while (!Serial);
    ledOff();
}

void loop()
{
    uint64_t timestamp = micros();

    // Read commands from serial
    while (Serial.available()) {
        uint8_t nextByte = Serial.read();
        if (nextByte == -1) {
            continue;
        }
        buffer[bytesPending] = static_cast<uint8_t>(nextByte);
        bytesPending++;
        if (bytesPending >= RX_MESSAGE_LENGTH) {
            // A complete message has been read into the buffer
            if (crc16(&buffer[2], RX_MESSAGE_LENGTH - 2) == deserialize<uint16_t>(&buffer[0])) {
                // Message is valid
                digitalWrite(LED_BUILTIN, HIGH); // Turn on LED to indicate a valid message was received
                uint8_t command = deserialize<uint8_t>(&buffer[2]);
                lastMessageTime = timestamp;
                bytesPending = 0;
                
                switch (command) {
                    case COMMAND_MOVE:
                        {
                            int32_t pitchSpeed = deserialize<int32_t>(&buffer[3]);
                            int32_t yawSpeed = deserialize<int32_t>(&buffer[7]);
                            move(pitchSpeed, yawSpeed);
                        }
                        break;
                    case COMMAND_HOME:
                        {
                            uint32_t pitchSpeed = deserialize<uint32_t>(&buffer[3]);
                            uint32_t yawSpeed = deserialize<uint32_t>(&buffer[7]);
                            home(pitchSpeed, yawSpeed);
                        }
                        break;
                    case COMMAND_OPEN_BREACH:
                        openBreach(true);
                        break;
                    case COMMAND_CLOSE_BREACH:
                        closeBreach();
                        break;
                    case COMMAND_RELOAD:
                        reload();
                        break;
                    case COMMAND_FIRE:
                        fire(true);
                        break;
                    case COMMAND_FIRE_AND_RELOAD:
                        fire(false);
                        reload();
                        break;
                    case COMMAND_MOTORS_ON:
                        pitch.setEnabled(true);
                        yaw.setEnabled(true);
                        break;
                    case COMMAND_MOTORS_OFF:
                        pitch.setEnabled(false);
                        yaw.setEnabled(false);
                        pitch.emergencyStop();
                        yaw.emergencyStop();
                        break;
                    default:
                        break;
                }
            } else {
                // CRC failed, skip one byte
                --bytesPending;
                memmove(&buffer[0], &buffer[1], bytesPending);
            }
        }
    }

    if (timeDiff(timestamp, lastMessageTime) > 500000) {
        // Haven't received a message 0.5 seconds
        pitch.setMaxSpeed(0);
        yaw.setMaxSpeed(0);
        digitalWrite(LED_BUILTIN, LOW);
    }
    
    if (timeDiff(timestamp, lastResponseTime) >= RESPONSE_INTERVAL) {
        sendMessage();
        lastResponseTime = timestamp;
    }
    poll();
}

void sendMessage() {
    Status status = STATUS_READY;
    if (homing_failed) {
        status = STATUS_HOMING_FAILED;
    } else if (!pitch.isEnabled() || !yaw.isEnabled()) {
        status = STATUS_MOTORS_OFF;
    } else if (homing) {
        status = STATUS_HOMING;
    } else if (!homed) {
        status = STATUS_HOMING_REQUIRED;
    } else if (reloading) {
        status = STATUS_RELOADING;
    } else if (breachOpen) {
        status = STATUS_BREACH_OPEN;
    } else if (!loaded) {
        status = STATUS_NOT_LOADED;
    }
    
    // Write status
    serialize(&buffer[2], static_cast<uint8_t>(status));
    // Write current position
    serialize(&buffer[3], static_cast<uint32_t>(pitch.getPosition() > 0 ? pitch.getPosition() : 0));
    serialize(&buffer[7], static_cast<uint32_t>(yaw.getPosition() > 0 ? yaw.getPosition() : 0));
    // Write CRC
    serialize(&buffer[0], crc16(&buffer[2], TX_MESSAGE_LENGTH - 2));
    
    for (int i = 0; i < TX_MESSAGE_LENGTH; ++i) {
        poll();
        Serial.write(buffer[i]);
    }
}
