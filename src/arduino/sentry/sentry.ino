#include <Arduino.h>
#include "serialize.h"
#include "motion.h"
#include "pins.h"
#include "Command.h"
#include "StepperDriver.h"

using namespace Sentry;

StepperDriver pitch(PITCH_STEP_PIN, PITCH_DIR_PIN, PITCH_ENABLE_PIN);
StepperDriver yaw(YAW_STEP_PIN, YAW_DIR_PIN, YAW_ENABLE_PIN);
StepperDriver slide(SLIDE_STEP_PIN, SLIDE_DIR_PIN, SLIDE_ENABLE_PIN);

const uint32_t BUFFER_LENGTH = 128;
const uint32_t RX_MESSAGE_LENGTH = 11;
const uint32_t TX_MESSAGE_LENGTH = 10;
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
    pitch.move(PITCH_MAX_STEPS * (PITCH_HOME_INVERTED ? -2 : 2));
    yaw.move(YAW_MAX_STEPS * (YAW_HOME_INVERTED ? -2 : 2));

    while (pitch.getDistanceToGo() != 0 || yaw.getDistanceToGo() != 0) {
        if (isEndstopHit(PITCH_ENDSTOP_PIN)) {
            pitch.setPosition(0);
            pitch.emergencyStop();
        }
        if (isEndstopHit(YAW_ENDSTOP_PIN)) {
            pitch.setPosition(0);
            pitch.emergencyStop();
        }
        pitch.poll();
        yaw.poll();
    }

    pitch.moveTo(PITCH_HOME_OFFSET);
    yaw.moveTo(YAW_HOME_OFFSET);

    while (pitch.getDistanceToGo() != 0 || yaw.getDistanceToGo() != 0) {
        pitch.poll();
        yaw.poll();
    }
}

void moveSlideToMM(float mm)
{
    slide.moveTo(mm * SLIDE_STEPS_PER_MM);
    slide.wait();
}

void openBreach()
{
    moveSlideToMM(20);
}

void closeBreach()
{
    moveSlideToMM(2);
}

void fire()
{
    moveSlideToMM(-2); // Overshoot to ensure the release is triggered
    slide.setPosition(0);
    slide.moveTo(0);
}

uint64_t timeDiff(uint64_t current, uint64_t previous)
{
    return current >= previous ? current - previous : current + (UINT64_MAX - previous);
}

void setup()
{
    ledBitmask = digitalPinToBitMask(LED_PIN);
    ledRegister = portOutputRegister(digitalPinToPort(LED_PIN));

    pinMode(PITCH_ENDSTOP_PIN, INPUT_PULLUP);
    pinMode(YAW_ENDSTOP_PIN, INPUT_PULLUP);

    pitch.setMaxSpeed(0);
    pitch.setAcceleration(20000);

    yaw.setMaxSpeed(0);
    yaw.setAcceleration(15000);

    slide.setMaxSpeed(1000);
    slide.setAcceleration(10000);

    Serial.begin(115200);
    while (!Serial);
}

void loop()
{
    uint64_t timestamp = micros();

    // Read commands from serial
    while (Serial.available()) {
        bytesPending += Serial.readBytes(buffer + bytesPending, 1);
        poll();
        if (bytesPending >= RX_MESSAGE_LENGTH) {
            // A complete message has been read into the buffer
            if (crc16(&buffer[2], RX_MESSAGE_LENGTH - 2) == deserialize<uint16_t>(&buffer[0])) {
                // Message is valid
                ledOn(); // Turn on LED to indicate a valid message was receive
                uint8_t command = deserialize<uint8_t>(&buffer[2]);
                int32_t pitchSpeed = deserialize<int32_t>(&buffer[3]);
                int32_t yawSpeed = deserialize<int32_t>(&buffer[7]);

                lastMessageTime = timestamp;
                bytesPending -= RX_MESSAGE_LENGTH;
                if (bytesPending > 0) {
                    memmove(buffer, buffer + RX_MESSAGE_LENGTH, BUFFER_LENGTH - bytesPending);
                }
                pitch.setMaxSpeed(static_cast<float>(abs(pitchSpeed)));
                pitch.moveTo(pitchSpeed >= 0 ? PITCH_MAX_STEPS : 0);
                yaw.setMaxSpeed(static_cast<float>(abs(yawSpeed)));
                yaw.moveTo(yawSpeed >= 0 ? YAW_MAX_STEPS : 0);

                poll();

                switch (command) {
                    case COMMAND_HOME:
                        home();
                        break;
                    case COMMAND_OPEN_BREACH:
                        openBreach();
                        break;
                    case COMMAND_CLOSE_BREACH:
                        closeBreach();
                        break;
                    case COMMAND_CYCLE_BREACH:
                        openBreach();
                        closeBreach();
                        break;
                    case COMMAND_FIRE:
                        fire();
                        break;
                    case COMMAND_FIRE_AND_CYCLE_BREACH:
                        fire();
                        openBreach();
                        closeBreach();
                        break;
                    case COMMAND_NONE:
                    default:
                        break;
                }
            } else {
                // CRC failed, skip one byte
                memmove(buffer, buffer + 1, --bytesPending);
                poll();
            }
        }
    }

    if (timeDiff(timestamp, lastMessageTime) > 500000) {
        // Haven't received a message 0.5 seconds
        pitch.setMaxSpeed(0);
        yaw.setMaxSpeed(0);
        ledOff();
    }
    
    if (timeDiff(timestamp, lastResponseTime) >= RESPONSE_INTERVAL) {
        // Write current position to serial
        
        serialize(&buffer[2], static_cast<uint32_t>(pitch.getPosition() > 0 ? pitch.getPosition() : 0));
        serialize(&buffer[6], static_cast<uint32_t>(yaw.getPosition() > 0 ? yaw.getPosition() : 0));
        serialize(&buffer[0], crc16(&buffer[2], TX_MESSAGE_LENGTH - 2));
        
        for (int i = 0; i < TX_MESSAGE_LENGTH; ++i) {
            poll();
            Serial.write(buffer[i]);
        }
        lastResponseTime = timestamp;
    }
    poll();
}
