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
// turtlc_start(json_config, threaded) -> i32
//   json_config:
//     a C string (null-terminated) holding JSON configuration
//   threaded:
//     if 1, we run the turtl core in a background thread and return immediately
//     after starting. if 0, turtlc_start will block until the core has exited.
//   -> returns 0 on success
TURTL_EXPORT int32_t TURTL_CONV turtlc_start(const char*, uint8_t);

// turtlc_send(message_bytes, message_len) -> i32
//   message_bytes:
//     a pointer to a block of u8 binary data holding a message to turtl
//   message_len:
//     the length in bytes of `message_bytes`
//   -> returns 0 on success
TURTL_EXPORT int32_t TURTL_CONV turtlc_send(const uint8_t*, size_t);

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
//     
TURTL_EXPORT uint8_t* TURTL_CONV turtlc_recv(uint8_t, const char*, size_t*);

// turtlc_recv_event(non_block, &msg_len) -> *uint8_t
//   non_block:
//     if 1, returns immediately if there are no messages to retrieve. if 0,
//     block until a message becomes available
//   msg_len:
//     a pointer to a size_t that is filled in with the length (in bytes) of the
//     event we receive
//   -> returns a pointer to our event data, or null if no event is available
//     and we set non_block = 1
//     
TURTL_EXPORT uint8_t* TURTL_CONV turtlc_recv_event(uint8_t, size_t*);

// turtlc_free(msg_ptr, len) -> i32
//   msg_ptr:
//     a pointer to a message we received from `turtlc_recv` or `turtlc_recv_event`
//   msg_len:
//     the length of the data in the pointer to our message (this len value is
//     also handed back to us from the recv functions as the `msg_len` param).
//   -> returns 0 on success
TURTL_EXPORT int32_t TURTL_CONV turtlc_free(const uint8_t*, size_t);

#ifdef __cplusplus
}		// extern "C" { ... }
#endif

#endif //_TURTLH_

