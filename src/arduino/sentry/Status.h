#ifndef SENTRY_STATUS_H
#define SENTRY_STATUS_H

namespace Sentry {

typedef enum : uint8_t {
    STATUS_READY = 100,
    STATUS_NOT_LOADED,
    STATUS_MAGAZINE_RELEASED,
    STATUS_RELOADING,
    STATUS_HOMING_REQUIRED,
    STATUS_HOMING,
    STATUS_MOTORS_OFF,
    STATUS_HOMING_FAILED,
} Status;

} // namespace Sentry

#endif // SENTRY_STATUS_H