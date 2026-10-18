// Embeds wxscr.manifest (visual styles for the settings dialog + per-monitor DPI awareness).
fn main() {
    println!("cargo:rerun-if-changed=wxscr.manifest");
    println!("cargo:rerun-if-changed=wxscr.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("wxscr.rc", embed_resource::NONE).manifest_optional().unwrap();
}
