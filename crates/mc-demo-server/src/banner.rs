//! ASCII banner for `mc start` — per ADR-0019 Decision 5.
//!
//! Uses raw ANSI escape codes for maximum terminal compatibility.

// ANSI color codes
const CYAN: &str = "\x1b[36m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Print the Mosaic banner to stdout with color.
pub fn print_banner() {
    println!();
    println!("{BOLD}{CYAN}  ███╗   ███╗ ██████╗ ███████╗ █████╗ ██╗ ██████╗{RESET}");
    println!("{BOLD}{CYAN}  ████╗ ████║██╔═══██╗██╔════╝██╔══██╗██║██╔════╝{RESET}");
    println!("{BOLD}{CYAN}  ██╔████╔██║██║   ██║███████╗███████║██║██║     {RESET}");
    println!("{BOLD}{CYAN}  ██║╚██╔╝██║██║   ██║╚════██║██╔══██║██║██║     {RESET}");
    println!("{BOLD}{CYAN}  ██║ ╚═╝ ██║╚██████╔╝███████║██║  ██║██║╚██████╗{RESET}");
    println!("{BOLD}{CYAN}  ╚═╝     ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝ ╚═════╝{RESET}");
    println!();
    println!("  {BOLD}Large Numbers Model{RESET} {DIM}· v0.1.0{RESET}");
    println!("  {DIM}912 tests · Formula engine complete{RESET}");
    println!();
}
