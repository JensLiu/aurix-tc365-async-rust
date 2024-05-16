use core::fmt::Write;

pub fn abort() -> ! {
    bw_r_drivers_tc37x::uart::print("aborted");
    loop {}
}

pub struct UartOut {}
impl UartOut {
    pub fn new() -> Self {
        Self {}
    }
}
impl Write for UartOut {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        bw_r_drivers_tc37x::uart::print(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print
{
	($($args:tt)+) => ({
			use core::fmt::Write;
			let _ = write!(crate::utils::UartOut::new(), $($args)+);
	});
}
