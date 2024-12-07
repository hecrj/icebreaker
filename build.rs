pub fn main() {
    println!("cargo::rerun-if-changed=fonts/icebreaker-icons.toml");
    iced_fontello::build("fonts/icebreaker-icons.toml").expect("Build icons font");
}
