fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=nff.ico");
    println!("cargo:rerun-if-changed=assets/nvencFFX.manifest");

    #[cfg(windows)]
    {
        // FileVersion / ProductVersion come from `package.version`.
        let mut res = winresource::WindowsResource::new();
        res.set_icon("nff.ico")
            .set_manifest_file("assets/nvencFFX.manifest")
            .set("ProductName", "nvencFFX")
            .set("FileDescription", "nvencFFX")
            .set("CompanyName", "Deepseek Enjoyer")
            .set("LegalCopyright", "Copyright (C) 2026 hadouken")
            .set("OriginalFilename", "nvencFFX.exe")
            .set("InternalName", "nvencFFX");
        if let Err(e) = res.compile() {
            println!("cargo:warning=Failed to compile Windows resource: {e}");
        }
    }
}
