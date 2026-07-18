const STARTUP_MESSAGE: &str = "RoadSim CLI foundation";

fn main() {
    println!("{STARTUP_MESSAGE}");
}

#[cfg(test)]
mod tests {
    use super::STARTUP_MESSAGE;

    #[test]
    fn startup_message_identifies_cli() {
        assert!(STARTUP_MESSAGE.contains("CLI"));
    }
}
