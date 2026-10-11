use std::{
    io::{self, Write},
    process::Command,
    time::Duration,
};
fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("args") => {
            println!("{}", args[2..].join("|"));
        }
        Some("env") => {
            for (k, v) in std::env::vars() {
                println!("{k}={v}");
            }
        }
        Some("flood") => {
            for _ in 0..200 {
                io::stdout().write_all(&[b'a'; 8192]).unwrap();
                io::stderr().write_all(&[b'b'; 8192]).unwrap();
            }
        }
        Some("sleep") => std::thread::sleep(Duration::from_secs(60)),
        Some("tree") => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .arg("sleep")
                .spawn()
                .unwrap();
            println!("{}", child.id());
            io::stdout().flush().unwrap();
            let _ = child.wait();
        }
        Some("lines") => {
            println!("ação 日本語\nsecond\nthird");
            eprintln!("stderr");
        }
        Some("escape") => print!("\x1b]52;c;payload\x07ação\x1b[31m日本語\u{202e}\nold\rnew"),
        _ => std::process::exit(7),
    }
}
