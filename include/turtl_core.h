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
TURTL_EXPORT int32_t TURTL_CONV turtl_start(const char *);
TURTL_EXPORT int32_t TURTL_CONV carrier_send(const char*, uint8_t*, size_t);
TURTL_EXPORT uint8_t* TURTL_CONV carrier_recv_nb(char*, uint64_t*);
TURTL_EXPORT uint8_t* TURTL_CONV carrier_recv(char*, uint64_t*);
TURTL_EXPORT int32_t TURTL_CONV carrier_free(uint8_t*);

#ifdef __cplusplus
}		// extern "C" { ... }
#endif

#endif //_TURTLH_

