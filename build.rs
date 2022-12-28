use vergen::TimestampKind;
use winresource::WindowsResource;

fn main() {
    // compile mute_control

    println!("cargo:rerun-if-changed=src/mute_control.rs");
    println!("cargo:rerun-if-changed=src/mute_control.hpp");
    println!("cargo:rerun-if-changed=src/mute_control.cpp");

    cxx_build::bridge("src/mute_control.rs").compile("annie-ffi");

    // embed manifest + icon

    println!("cargo:rerun-if-changed=resource/annie-main.ico");
    WindowsResource::new()
        .set_icon("resource/annie-main.ico")
        .set("ProductName", "Annie")
        .set("FileDescription", "Annie")
        .compile()
        .unwrap();

    // generate build info

    let mut config = vergen::Config::default();
    *config.build_mut().kind_mut() = TimestampKind::DateOnly;
    vergen::vergen(config).unwrap();
}
