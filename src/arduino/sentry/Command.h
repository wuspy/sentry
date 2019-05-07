#ifndef SENTRY_COMMAND_H
#define SENTRY_COMMAND_H

namespace Sentry {

typedef enum : uint8_t {
    COMMAND_MOVE = 200,
    COMMAND_HOME,
    COMMAND_OPEN_BREACH,
    COMMAND_CLOSE_BREACH,
    COMMAND_RELOAD,
    COMMAND_FIRE,
    COMMAND_FIRE_AND_RELOAD,
    COMMAND_MOTORS_ON,
    COMMAND_MOTORS_OFF,
} Command;

} // namespace Sentry

#endif // SENTRY_COMMAND_H