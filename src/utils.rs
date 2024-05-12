use bw_r_drivers_tc37x::uart::print;

pub fn abort() -> ! {
    print("aborted");
    loop {}
}