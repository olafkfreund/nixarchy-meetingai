fn main() {
    println!("cargo:rerun-if-changed=packaging/windows/lennette.rc");
    println!("cargo:rerun-if-changed=src/assets/lennette.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("packaging/windows/lennette.rc", embed_resource::NONE)
            .manifest_required()
            .expect("embed the Lennette application icon");
    }
}
