fn main() {
    // Re-embed icons when they change
    println!("cargo:rerun-if-changed=icons/");
    tauri_build::build()
}
