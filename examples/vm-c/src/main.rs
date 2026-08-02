//! Compiles a program and runs it on `nh-vm`.
use vm_c::{compile, run};

const SAMPLE: &str = r#"
x = 10;
y = 4;
print x + y * 2;
print -(x - y);
print x > y;
print 12 & 10;
if (x > y) { print 1; }
if (y > x) { print 999; }
n = 0;
while (n < 3) { print n; n = n + 1; }
"#;

fn main() {
    let program = compile(SAMPLE).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    println!("--- {} instructions ---", program.code.len());
    for line in run(&program).unwrap() {
        println!("{line}");
    }
}
