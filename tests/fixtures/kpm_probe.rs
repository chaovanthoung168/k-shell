//! Test-owned native executable. Never downloads or installs real software.
use std::{env,fs,path::PathBuf};
fn main(){
    let exe=env::current_exe().unwrap();let parent=exe.parent().unwrap();let home=if parent.file_name().unwrap()=="bin"{parent.parent().unwrap()}else{parent};
    let version=fs::read_to_string(home.join("fixture-version.txt")).unwrap();let base=version.split('+').next().unwrap();
    let name=exe.file_stem().unwrap().to_str().unwrap();let args:Vec<_>=env::args().skip(1).collect();
    if args.first().is_some_and(|s|s=="--probe") {
        let target=PathBuf::from(&args[1]);let path=env::var_os("PATH").and_then(|p|env::split_paths(&p).next()).unwrap_or_default();
        fs::write(target,format!("name={name}\nversion={version}\nexecutable={}\nJAVA_HOME={}\nRUSTUP_HOME={}\nCARGO_HOME={}\nfirst_path={}\n",exe.display(),env::var("JAVA_HOME").unwrap_or_default(),env::var("RUSTUP_HOME").unwrap_or_default(),env::var("CARGO_HOME").unwrap_or_default(),path.display())).unwrap();return;
    }
    match name {
        "java"=>eprintln!("openjdk version \"{base}\"\nOpenJDK Runtime Environment Temurin (build {version})"),
        "javac"=>eprintln!("javac {base}"),"node"=>println!("v{version}"),"rustc"=>println!("rustc {version} (test fixture)"),"cargo"=>println!("cargo {version} (test fixture)"),_=>std::process::exit(2)
    }
}
