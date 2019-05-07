#ifndef SENTRY_MOTION_H
#define SENTRY_MOTION_H

#include <math.h>

namespace Sentry {

const int32_t PITCH_STEPS_PER_REV   = 4 * 200;
const int32_t YAW_STEPS_PER_REV     = 4 * 200;
const int32_t SLIDE_STEPS_PER_REV   = 2 * 200;

const float SLIDE_GEAR_MODULE       = 1.2 * cos(30);
const int32_t SLIDE_PINION_TEETH    = 8;
const float SLIDE_STEPS_PER_MM      = (float)SLIDE_STEPS_PER_REV / (SLIDE_PINION_TEETH * SLIDE_GEAR_MODULE * 3.14159);
const int32_t SLIDE_MAX_MM          = 23;
const int32_t SLIDE_PRIME_MM        = 3;
const bool SLIDE_INVERTED           = true;

const int32_t PITCH_PINION_TEETH    = 8;
const int32_t PITCH_GEAR_TEETH      = 72;
const float PITCH_GEAR_RATIO        = (float)PITCH_GEAR_TEETH / (float)PITCH_PINION_TEETH;
const int32_t PITCH_MAX_DEGREES     = 53;
const int32_t PITCH_MIN_DEGREES     = -72;
const int32_t PITCH_MAX_STEPS       = PITCH_GEAR_RATIO * PITCH_STEPS_PER_REV * (PITCH_MAX_DEGREES - PITCH_MIN_DEGREES) / 360;
const int32_t PITCH_HOME_OFFSET     = 0;
const bool PITCH_HOME_INVERTED      = true;
const bool PITCH_HOME_BIDIRECTIONAL = false;
const bool PITCH_INVERTED           = false;

const int32_t YAW_PINION_TEETH      = 8;
const int32_t YAW_GEAR_TEETH        = 84;
const float YAW_GEAR_RATIO          = (float)YAW_GEAR_TEETH / (float)YAW_PINION_TEETH;
const int32_t YAW_MIN_DEGREES       = 0;
const int32_t YAW_MAX_DEGREES       = 352;
const int32_t YAW_MAX_STEPS         = YAW_GEAR_RATIO * YAW_STEPS_PER_REV * (YAW_MAX_DEGREES - YAW_MIN_DEGREES) / 360;
const int32_t YAW_HOME_OFFSET       = YAW_GEAR_RATIO * YAW_STEPS_PER_REV / 4.5;
const bool YAW_HOME_INVERTED        = true;
const bool YAW_HOME_BIDIRECTIONAL   = true;
const bool YAW_INVERTED             = true;

} // namespace Sentry

#endif // SENTRY_MOTION_H