#ifndef SENTRY_PINS_H
#define SENTRY_PINS_H

namespace Sentry {

const uint8_t PITCH_STEP_PIN        = 54;
const uint8_t PITCH_DIR_PIN         = 55;
const uint8_t PITCH_ENABLE_PIN      = 38;
const uint8_t PITCH_CS_PIN          = 59;
const uint8_t PITCH_ENDSTOP_PIN     = 3;

const uint8_t YAW_STEP_PIN          = 60;
const uint8_t YAW_DIR_PIN           = 61;
const uint8_t YAW_ENABLE_PIN        = 56;
const uint8_t YAW_CS_PIN            = 63;
const uint8_t YAW_ENDSTOP_PIN       = 14;

const uint8_t SLIDE_STEP_PIN        = 26;
const uint8_t SLIDE_DIR_PIN         = 28;
const uint8_t SLIDE_ENABLE_PIN      = 24;
const uint8_t SLIDE_CS_PIN          = 42;

} // namespace Sentry

#endif // SENTRY_PINS_H