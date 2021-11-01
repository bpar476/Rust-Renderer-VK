use std::io::Write;
use std::{
    env,
    fs::{self, File},
    option,
    path::Path,
};

use shaderc::{self, ShaderKind};
use walkdir::WalkDir;

const SHADER_DIR: &str = "src/shaders";

fn main() {
    println!("cargo:rerun-if-changed=src/shaders");

    let out_dir = env::var("OUT_DIR").unwrap();
    // TODO Error handling

    let mut compiler = shaderc::Compiler::new().unwrap();
    let mut options = shaderc::CompileOptions::new().unwrap();
    options.add_macro_definition("EP", Some("main"));

    for entry in WalkDir::new(SHADER_DIR) {
        let unwrapped = entry.unwrap();
        if unwrapped.file_type().is_dir() {
            continue;
        }
        let path = unwrapped.path();

        println!("Compiling {}", path.display());

        let source =
            fs::read_to_string(path).expect(format!("Reading {}", path.display()).as_str());

        let file_name = path
            .file_name()
            .expect("shader file name")
            .to_str()
            .expect("shader file name invalid unicode");
        let shader_kind = if file_name.contains("vert") {
            ShaderKind::Vertex
        } else if file_name.contains("frag") {
            ShaderKind::Fragment
        } else {
            panic!("Unrecognised shader kind {}", file_name)
        };

        let binary_result = compiler.compile_into_spirv(
            source.as_str(),
            shader_kind,
            file_name,
            "main",
            Some(&options),
        );
        let split_name = file_name.split(".").collect::<Vec<&str>>();
        let base = split_name[0];
        match binary_result {
            Ok(artifact) => {
                println!("Compiled {} as {:?}", file_name, shader_kind);

                let output_file_path = Path::new(out_dir.as_str()).join(format!("{}.spv", base));
                let mut output_file =
                    File::create(&output_file_path).expect("creating shader output file");
                match output_file.write(artifact.as_binary_u8()) {
                    Ok(n) => {
                        // TODO print warnings
                        println!("Wrote {} bytes to {}", n, output_file_path.display());
                    }
                    Err(e) => {
                        panic!("unable to write shader binary: {}", e.to_string())
                    }
                }
            }
            Err(e) => {
                panic!("unable to compile shader binary {}", e.to_string())
            }
        }
    }
}
