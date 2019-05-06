#ifndef SENTRY_STATUS_H
#define SENTRY_STATUS_H

namespace Sentry {

typedef enum : uint8_t {
    STATUS_READY = 100,
    STATUS_HOMING_REQUIRED,
    STATUS_HOMING,
    STATUS_MOTORS_OFF,
    STATUS_ERROR,
} Status;

} // namespace Sentry

#endif // SENTRY_STATUS_H