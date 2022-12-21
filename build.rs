use cc::Build;
use vergen::TimestampKind;
use winres::WindowsResource;

fn main() {
    // compile mute_control

    println!("cargo:rerun-if-changed=src/mute_control.hpp");
    println!("cargo:rerun-if-changed=src/mute_control.cpp");
    Build::new()
        .file("src/mute_control.cpp")
        .warnings(true)
        .warnings_into_errors(true)
        .compile("mute_control");

    // compile resources

    println!("cargo:rerun-if-changed=resource/annie-main.ico");
    WindowsResource::new()
        .set_icon("resource/annie-main.ico")
        .compile()
        .unwrap();

    // generate build info

    let mut config = vergen::Config::default();
    *config.build_mut().kind_mut() = TimestampKind::DateOnly;
    vergen::vergen(config).unwrap();
}
