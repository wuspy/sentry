#include <Arduino.h>
#include "serialize.h"
#include "motion.h"
#include "pins.h"
#include "Command.h"
#include "Status.h"
#include "StepperDriver.h"

using namespace Sentry;

StepperDriver pitch(PITCH_STEP_PIN, PITCH_DIR_PIN, PITCH_ENABLE_PIN, PITCH_INVERTED);
StepperDriver yaw(YAW_STEP_PIN, YAW_DIR_PIN, YAW_ENABLE_PIN, YAW_INVERTED);
StepperDriver slide(SLIDE_STEP_PIN, SLIDE_DIR_PIN, SLIDE_ENABLE_PIN, SLIDE_INVERTED);

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

Status status = STATUS_HOMING_REQUIRED;
bool loaded = false;

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

void home()
{
    pitch.setEnabled(true);
    yaw.setEnabled(true);

    // Stop motors
    pitch.moveTo(0);
    yaw.moveTo(0);
    pitch.setPosition(0);
    yaw.setPosition(0);
    while (pitch.getDistanceToGo() != 0 || yaw.getDistanceToGo() != 0) {
        poll();
    }

    // Move until endstops trigger
    pitch.moveTo(PITCH_MAX_STEPS * (PITCH_HOME_INVERTED ? 2 : -2));
    yaw.moveTo(YAW_MAX_STEPS * (YAW_HOME_INVERTED ? -2 : 2));
    while (pitch.getDistanceToGo() != 0 || yaw.getDistanceToGo() != 0) {
        if (isEndstopHit(PITCH_ENDSTOP_PIN)) {
            pitch.setPosition(0);
            pitch.emergencyStop();
        }
        if (isEndstopHit(YAW_ENDSTOP_PIN)) {
            yaw.setPosition(0);
            yaw.emergencyStop();
        }
        poll();
    }

    // Move axes to 0 degrees
    pitch.moveTo(PITCH_HOME_OFFSET);
    yaw.moveTo(YAW_HOME_OFFSET);
    while (pitch.getDistanceToGo() != 0 || yaw.getDistanceToGo() != 0) {
        poll();
    }
    pitch.setPosition(PITCH_HOME_INVERTED ? PITCH_MAX_STEPS : 0);
    pitch.moveTo(pitch.getPosition());
    yaw.setPosition(YAW_HOME_INVERTED ? YAW_MAX_STEPS : 0);
    yaw.moveTo(yaw.getPosition());
}

void openBreach(bool disable)
{
    if (!loaded && status != STATUS_MOTORS_OFF) {
        slide.setEnabled(true);
        slide.moveTo(3.3 * SLIDE_STEPS_PER_REV);
        slide.wait();
        if (disable) {
            slide.setEnabled(false);
        }
    }
}

void closeBreach()
{
    if (!loaded && status != STATUS_MOTORS_OFF) {
        slide.setEnabled(true);
        slide.moveTo(0.6 * SLIDE_STEPS_PER_REV);
        slide.wait();
        slide.setEnabled(false);
        loaded = true;
    }
}

void fire(bool disable)
{
    if (loaded && status != STATUS_MOTORS_OFF) {
        slide.setEnabled(true);
        slide.moveTo(0);
        slide.wait();
        if (disable) {
            slide.setEnabled(false);
        }
        loaded = false;
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
    pinMode(2, INPUT_PULLUP);
    pinMode(15, INPUT_PULLUP);
    pinMode(19, INPUT_PULLUP); 

    pitch.setMaxSpeed(0);
    pitch.setAcceleration(20000);
    pitch.setEnabled(false);

    yaw.setMaxSpeed(0);
    yaw.setAcceleration(15000);
    yaw.setEnabled(false);

    slide.setEnabled(false);
    slide.setAcceleration(40000);
    slide.setMaxSpeed(2200);

    ledOn();
    Serial.begin(115200);
    while (!Serial);
    ledOff();
}

void loop()
{
    uint64_t timestamp = micros();

    if (digitalRead(2) == LOW) {
        fire(false);
        openBreach(false);
        closeBreach();
    }
    if (digitalRead(19) == LOW) {
        openBreach(true);
    }
    if (digitalRead(15) == LOW) {
        closeBreach();
    }

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
                        if (status == STATUS_READY) {
                            int32_t pitchSpeed = deserialize<int32_t>(&buffer[3]);
                            int32_t yawSpeed = deserialize<int32_t>(&buffer[7]);
                            pitch.setMaxSpeed(static_cast<float>(abs(pitchSpeed)));
                            pitch.moveTo(pitchSpeed >= 0 ? PITCH_MAX_STEPS : 0);
                            yaw.setMaxSpeed(static_cast<float>(abs(yawSpeed)));
                            yaw.moveTo(yawSpeed >= 0 ? YAW_MAX_STEPS : 0);
                        }
                        break;
                    case COMMAND_HOME:
                        if (status != STATUS_MOTORS_OFF && status != STATUS_ERROR) {
                            uint32_t pitchSpeed = deserialize<uint32_t>(&buffer[3]);
                            uint32_t yawSpeed = deserialize<uint32_t>(&buffer[7]);
                            pitch.setMaxSpeed(static_cast<float>(pitchSpeed));
                            yaw.setMaxSpeed(static_cast<float>(yawSpeed));
                            status = STATUS_HOMING;
                            sendMessage();
                            home();
                            status = STATUS_READY;
                        }
                        break;
                    case COMMAND_OPEN_BREACH:
                        openBreach(true);
                        break;
                    case COMMAND_CLOSE_BREACH:
                        closeBreach();
                        break;
                    case COMMAND_CYCLE_BREACH:
                        openBreach(false);
                        closeBreach();
                        break;
                    case COMMAND_FIRE:
                        fire(true);
                        break;
                    case COMMAND_FIRE_AND_CYCLE_BREACH:
                        fire(false);
                        openBreach(false);
                        closeBreach();
                        break;
                    case COMMAND_MOTORS_ON:
                        if (status == STATUS_MOTORS_OFF) {
                            pitch.setEnabled(true);
                            yaw.setEnabled(true);
                            status = STATUS_HOMING_REQUIRED;
                        }
                        break;
                    case COMMAND_MOTORS_OFF:
                        pitch.setEnabled(false);
                        yaw.setEnabled(false);
                        pitch.emergencyStop();
                        yaw.emergencyStop();
                        status = STATUS_MOTORS_OFF;
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
