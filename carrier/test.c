#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern int32_t carrier_send(char*, uint8_t*, size_t);
extern uint8_t* carrier_recv_nb(char*, uint64_t*);
extern uint8_t* carrier_recv(char*, uint64_t*);
extern int32_t carrier_vacuum();
extern int32_t carrier_free(uint8_t*);

void send(int id, char* msg) {
	int32_t send = carrier_send("core", msg, strlen(msg));
	/*printf("send%d: %s\n", id, send == 0 ? "success" : "fail");*/
	fflush(stdout);
}

void recv(int id) {
	uint64_t len = 0;
	uint8_t* recv = carrier_recv_nb("core", &len);
	/*printf("recv%d: got %d bytes\n", id, len);*/
	fflush(stdout);
	if(recv && len > 0) {
		int32_t res = carrier_free(recv);
		/*printf("recv%d: free: %d\n", id, res);*/
		fflush(stdout);
	} else {
		printf("recv%d: no message received\n", id);
		fflush(stdout);
	}
}

int main() {
	int num = 9999;
	printf("start...\n", num);
	fflush(stdout);
	sleep(5);

	printf("sending %d\n", num);
	fflush(stdout);
	for(int i = 0; i < num; i++) {
		send(i, "hello, there");
	}
	printf("send done!\n");
	fflush(stdout);
	sleep(5);

	printf("receiving %d\n", num);
	fflush(stdout);
	for(int i = 0; i < num + 1; i++) {
		recv(i);
	}
	printf("recv done!\n");
	fflush(stdout);
	sleep(5);

	printf("vacuuming: %d\n", carrier_vacuum());
	fflush(stdout);
	sleep(5);

	printf("sending %d\n", (num * 8));
	fflush(stdout);
	for(int i = 0; i < (num * 8); i++) {
		send(i, "omg lol wtfFFFFFFFF!!!");
	}
	printf("send done!\n");
	fflush(stdout);
	sleep(5);

	printf("receiving %d\n", (num * 8));
	fflush(stdout);
	for(int i = 0; i < (num * 8); i++) {
		recv(i);
	}
	printf("recv done!\n");
	fflush(stdout);
	sleep(5);

	printf("vacuuming: %d\n", carrier_vacuum());
	fflush(stdout);
	sleep(5);

	return 0;
}

