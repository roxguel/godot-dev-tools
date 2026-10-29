use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("gdt: the godot shim is not implemented yet (see docs/roadmap.md, milestone M2)");
    ExitCode::from(2)
}
