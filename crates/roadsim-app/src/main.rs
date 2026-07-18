const STARTUP_MESSAGE: &str = "RoadSim desktop foundation";

fn main() {
    println!("{STARTUP_MESSAGE}");
}

#[cfg(test)]
mod tests {
    use super::STARTUP_MESSAGE;

    #[test]
    fn startup_message_identifies_desktop_application() {
        assert!(STARTUP_MESSAGE.contains("desktop"));
    }
}
