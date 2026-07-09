pub struct Code;

impl Code {
    pub const OK: i32 = 0;
    pub const CHANGES: i32 = 10;
    pub const ERROR: i32 = 1;
}

pub fn finish(result: Result<i32, String>) -> ! {
    match result {
        Ok(code) => std::process::exit(code),
        Err(msg) => {
            eprintln!("skillshield: error: {msg}");
            std::process::exit(Code::ERROR);
        }
    }
}
