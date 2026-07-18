mod demo;
mod shell;
mod simulation;

fn main() {
    if let Err(error) = shell::run() {
        eprintln!("RoadSim startup failed: {error}");
        std::process::exit(1);
    }
}
