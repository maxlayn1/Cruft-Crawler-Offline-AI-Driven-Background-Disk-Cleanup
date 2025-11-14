# Overview
Actors are defined in the actor/ directory. They have two related functions run() and internal_behaviour()
Actors can also have a function called simulated behaviour for testing purposes


## Minor Notes
- If you see || in a function call like actor.into_spotlight(|| ) that is a closure (rust feature) that allows you to pass functions into functions

## fn run(actor: SteadyStateShadow) -> Result<(),Box<dyn Error>>
intro:
```
/// Actor entry point function following the steady_state actor pattern.
/// Every actor must have a `run` function that accepts a SteadyActorShadow and returns
/// a Result. This function serves as the actor's lifecycle manager - if it returns
/// an error (or panics), the steady_state framework will automatically restart the actor,
/// providing fault tolerance without manual error handling.
```

the run function runs the internal behaviour function when called
both of these functions return a Result<> which handles errors if anything happens and aborts the program

```
// Transform the basic context into a monitoring-enabled SteadyActor.
// The empty arrays [] represent input and output channel configurations -
// this actor operates independently without inter-actor communication channels.
// Monitoring enables this actor to appear in telemetry dashboards with
// real-time metrics like CPU usage, and throughput.
// if we passed actor as-is, the code continues to work as expected, but without
// any telemetry or metrics collection overhead.
```
## async fn internal_behaviour(mut actor: A) -> Result<(), Box<dyn Error>>
```
/// Core actor behavior separated from monitoring concerns for testability.
/// This function accepts any type implementing SteadyActor, allowing the same
/// logic to run with or without monitoring enabled.
```


# Actor functions that you can call
- into_spotlight()
- into_shadow()
- is_running()
- request_shutdown()

## for sending messages and recieving Messages
- try_send()
- try_take()

# Actor Macro's that you can call
- await_for_all!:  calls await on every future passed in and then continue after they are all complete.
- await_for_any!:  calls await simultaneously on every future passed in and then continue after one of them has completed.
- await_for_all_or_proceed_upon!: same as await_for_all except that if the first item is done, it immediately continues.
- into_monitor!: takes a SteadyContext and all the rx/tx channels and creates a monitor of actors for telemetry (very important)

# module that exist for the Steady-State Library
actor_builder           Actor construction, configuration, and scheduling utilities.
builder                 Define Command line arguments
channel_builder         Channel construction and configuration utilities.
channel_builder_units   Telemetry details and unit structs for channels
clap_derive             clap_derive
distributed             Components and builders for distributed systems.
error                   Error reporting
expression_steady_eye   Utilities for inspecting short boolean sequences.
graph_testing           Utilities for testing full graphs of actors.
install                 This module contains submodules to support different installation strategies.
logging_util            Miscellaneous utility functions.
monitor                 Monitoring utilities for inspecting channel and actor metrics at runtime.
parser                  Command line argument parser
simulate_edge           Tools for simulating edge cases in testing.
state_management        Manage state for actors scros failures and restarts
steady_actor            Commands and utilities for channels used by actors.
steady_actor_shadow     Shadow utilities for steady actors.
steady_actor_spotlight  Spotlight utilities for steady actors.
steady_rx               Receiver channel features and utilities.
steady_tx               Transmitter channel features and utilities.
yield_now               Utilities for yielding execution within actors.
