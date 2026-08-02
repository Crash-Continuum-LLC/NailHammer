//! The same program as the C twin's, in BASIC.
use vm_basic::{compile, run};

const SAMPLE: &str = r#"
LET x = 10
LET y = 4
PRINT x + y * 2
PRINT -(x - y)
PRINT x > y
PRINT 12 AND 10
IF x > y THEN
PRINT 1
END IF
IF y > x THEN
PRINT 999
END IF
LET n = 0
WHILE n < 3
PRINT n
LET n = n + 1
WEND
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
