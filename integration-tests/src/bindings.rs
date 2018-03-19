// i made this
extern "C" {
    pub fn turtlc_start(config: *const ::std::os::raw::c_char, threaded: u8) -> i32;
    pub fn turtlc_send(message_bytes: *const u8, message_len: usize) -> i32;
    pub fn turtlc_recv(non_block: u8, msgid: *const ::std::os::raw::c_char, len: *mut usize) -> *const u8;
    pub fn turtlc_recv_event(non_block: u8, len: *mut usize) -> *const u8;
    pub fn turtlc_free(msg: *const u8, len: usize) -> i32;
}

