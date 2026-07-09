pub struct Code;

impl Code {
    #[allow(dead_code)]
    pub const OK: i32 = 0;
    #[allow(dead_code)]
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
