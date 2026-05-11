//! ASCII banner for `mc start` / `mc up` — per ADR-0019 Decision 5.
//!
//! Uses truecolor ANSI escape codes (24-bit) for a purple→blue gradient.
//! Falls back gracefully on terminals without truecolor (still readable, just uncolored).

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Truecolor foreground: \x1b[38;2;R;G;Bm
fn rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{r};{g};{b}m")
}

/// Purple→blue gradient across 6 lines.
/// Line 0: warm purple (147, 51, 234)
/// Line 5: cool blue  (59, 130, 246)
fn gradient_color(line: usize) -> String {
    let t = line as f32 / 5.0;
    let r = (147.0 + (59.0 - 147.0) * t) as u8;
    let g = (51.0 + (130.0 - 51.0) * t) as u8;
    let b = (234.0 + (246.0 - 234.0) * t) as u8;
    rgb(r, g, b)
}

/// Print the Mosaic banner to stdout with a purple→blue gradient.
pub fn print_banner() {
    let lines = [
        "  ███╗   ███╗ ██████╗ ███████╗ █████╗ ██╗ ██████╗",
        "  ████╗ ████║██╔═══██╗██╔════╝██╔══██╗██║██╔════╝",
        "  ██╔████╔██║██║   ██║███████╗███████║██║██║     ",
        "  ██║╚██╔╝██║██║   ██║╚════██║██╔══██║██║██║     ",
        "  ██║ ╚═╝ ██║╚██████╔╝███████║██║  ██║██║╚██████╗",
        "  ╚═╝     ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═╝ ╚═════╝",
    ];

    println!();
    for (i, line) in lines.iter().enumerate() {
        let color = gradient_color(i);
        println!("{BOLD}{color}{line}{RESET}");
    }
    println!();
    println!("  {BOLD}Large Numbers Model{RESET} {DIM}· v0.1.0{RESET}");
    println!("  {DIM}1091 tests · Narrative engine complete{RESET}");
    println!();
}
