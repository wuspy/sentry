package io.github.kibogaoka.sentry

enum class SentryState {
    READY,
    NOT_LOADED,
    BREACH_OPEN,
    RELOADING,
    HOMING_REQUIRED,
    HOMING,
    MOTORS_OFF,
    HOMING_FAILED,
    ERROR,
}
