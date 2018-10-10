#ifndef _TURTLH_

#define _TURTLH_

#ifdef _WIN32
	#define TURTL_EXPORT __declspec(dllexport)
	#define TURTL_CONV __cdecl
#else
	#define TURTL_EXPORT extern
	#define TURTL_CONV
#endif

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

// -----------------------------------------------------------------------------
// turtlc_start(json_config, threaded) -> i32
//   json_config:
//     a C string (null-terminated) holding JSON configuration
//   threaded:
//     if 1, we run the turtl core in a background thread and return immediately
//     after starting. if 0, turtlc_start will block until the core has exited.
//   -> returns 0 on success
// -----------------------------------------------------------------------------
// This initializes the Turtl core and gets it ready to start listening to
// incoming messages/commands.
TURTL_EXPORT int32_t TURTL_CONV turtlc_start(const char*, uint8_t);

// -----------------------------------------------------------------------------
// turtlc_send(msg_bytes, msg_len) -> i32
//   msg_bytes:
//     a pointer to a block of u8 binary data holding a message to turtl
//   msg_len:
//     the length in bytes of `message_bytes`
//   -> returns 0 on success
// -----------------------------------------------------------------------------
// Send a message to the Turtl core. Messages are JSON arrays in the format:
//   ["<msg id>", "command", [args, ...]]
//
// The core will respond using the following format (see `turtlc_recv()`)
//   {"id": "<msg id>", "e": 1|0, "d": ...}
//
// The <msg id> of the response will match the id that was sent in, this way you
// know which response is for which message. The `e` param will be 0 on success,
// 1 if there was an error, and `d` will hold the data for the response (if
// any).
TURTL_EXPORT int32_t TURTL_CONV turtlc_send(const uint8_t*, size_t);

// -----------------------------------------------------------------------------
// turtlc_recv(non_block, msgid, &msg_len) -> *uint8_t
//   non_block:
//     if 1, returns immediately if there are no messages to retrieve. if 0,
//     block until a message becomes available
//   msgid:
//     a c string (null temrinated) message id we want. can be null or "" if we
//     just want to grab the next available message
//   msg_len:
//     a pointer to a size_t that is filled in with the length (in bytes) of the
//     message we receive
//   -> returns a pointer to our message data, or null if no message is
//     available and we set non_block = 1
// -----------------------------------------------------------------------------
// Receive a response from the core. This will always be a response to a message
// that was sent with `turtlc_send()`.
//
// Note that if a null message is returned but msg_len > 0, this indicates an
// error occurred.
TURTL_EXPORT const uint8_t* TURTL_CONV turtlc_recv(uint8_t, const char*, size_t*);

// -----------------------------------------------------------------------------
// turtlc_recv_event(non_block, &msg_len) -> *uint8_t
//   non_block:
//     if 1, returns immediately if there are no messages to retrieve. if 0,
//     block until a message becomes available
//   msg_len:
//     a pointer to a size_t that is filled in with the length (in bytes) of the
//     event we receive
//   -> returns a pointer to our event data, or null if no event is available
//     and we set non_block = 1
// -----------------------------------------------------------------------------
// Receive an event from the core. Note that events are separate from responses
// because you can have many (or none) while the core is processing a command.
// Events are used to notify the UI of certain stages of execution being
// completed or certain conditions being met.
//
// Note that if a null message is returned but msg_len > 0, this indicates an
// error occurred.
TURTL_EXPORT const uint8_t* TURTL_CONV turtlc_recv_event(uint8_t, size_t*);

// -----------------------------------------------------------------------------
// turtlc_free(msg_ptr, len) -> i32
//   msg_ptr:
//     a pointer to a message we received from `turtlc_recv` or `turtlc_recv_event`
//   msg_len:
//     the length of the data in the pointer to our message (this len value is
//     also handed back to us from the recv functions as the `msg_len` param).
//   -> returns 0 on success
// -----------------------------------------------------------------------------
// `turtlc_recv()` and `turtlc_recv_event()` allocate memory when passing
// messages to you. You must free these messages when you are done with them
// by calling `turtlc_free()` on them.
TURTL_EXPORT int32_t TURTL_CONV turtlc_free(const uint8_t*, size_t);

// -----------------------------------------------------------------------------
// turtlc_lasterr() -> char*
//   -> returns a pointer to a null-terminated string of the last error that
//      occurred (null if no error). Must be freed via `turtlc_free_err`
// -----------------------------------------------------------------------------
// Grab the last error that occurred. This is usually used during initialization
// of the core, and is especially handy when working with platforms that gobble
// STDOUT (since initialization errors will also be logged). Mainly, you'd call
// this after getting a non-zero from `turtlc_start()`/`turtlc_send()`, or if
// `turtlc_recv()`/`turtlc_recv_event()` return a null with an msg_len > 0.
TURTL_EXPORT char* TURTL_CONV turtlc_lasterr();

// -----------------------------------------------------------------------------
// turtlc_free_err(lasterr) -> i32
//   lasterr:
//     a pointer to an error message returned from `turtlc_lasterr()`
//   -> returns 0 on success
// -----------------------------------------------------------------------------
// Free an error string returned from `turtlc_lasterr()`.
TURTL_EXPORT int32_t TURTL_CONV turtlc_free_err(char*);

#ifdef __cplusplus
}		// extern "C" { ... }
#endif

#endif //_TURTLH_

