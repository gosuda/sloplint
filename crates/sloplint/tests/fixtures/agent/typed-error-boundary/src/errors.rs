pub struct Config {
    pub name: String,
}

pub fn load_config(path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    std::fs::read_to_string(path).map(|text| Config { name: text })
}
