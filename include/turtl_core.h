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
TURTL_EXPORT int32_t TURTL_CONV turtlc_start(const char*, uint8_t);
TURTL_EXPORT int32_t TURTL_CONV turtlc_send(const uint8_t*, size_t);
TURTL_EXPORT uint8_t* TURTL_CONV turtlc_recv(uint8_t, const char*, size_t*);
TURTL_EXPORT uint8_t* TURTL_CONV turtlc_recv_event(uint8_t, size_t*);
TURTL_EXPORT int32_t TURTL_CONV turtlc_free(const uint8_t*, size_t);

#ifdef __cplusplus
}		// extern "C" { ... }
#endif

#endif //_TURTLH_

