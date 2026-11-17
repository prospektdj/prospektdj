use std::{
    env, fs,
    path::{Path, PathBuf},
};
fn collect(root: &Path, entries: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("license directory").flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect(&p, entries);
        } else {
            entries.push(p);
        }
    }
}
fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/icon.ico")
            .set("ProductName", "Prospekt DJ BETA")
            .set(
                "FileDescription",
                "Prospekt DJ — open source BETA DJ studio",
            );
        resource
            .compile()
            .expect("Windows application icon resource");
    }
    println!("cargo:rerun-if-changed=assets/icon.ico");

    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut files = vec![root.join("LICENSE")];
    collect(&root.join("licenses"), &mut files);
    for e in fs::read_dir(root.join("assets/fonts")).unwrap().flatten() {
        if e.file_name().to_string_lossy().contains("LICENSE") {
            files.push(e.path());
        }
    }
    files.sort();
    let mut code = String::from("pub const LICENSES: &[(&str,&str)] = &[\n");
    for path in files {
        let name = path
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        code.push_str(&format!(
            "({name:?},include_str!({:?})),\n",
            path.to_string_lossy()
        ));
    }
    code.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("licenses.rs"),
        code,
    )
    .unwrap();
    println!("cargo:rerun-if-changed=licenses");
    println!("cargo:rerun-if-changed=LICENSE");
    println!("cargo:rerun-if-changed=assets/fonts");
}
