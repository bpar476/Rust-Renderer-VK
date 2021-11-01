use std::{fs::File, io::Read, path::Path};

pub fn read_shader_code(shader_path: &Path) -> Vec<u32> {
    let spv_file =
        File::open(shader_path).expect(&format!("Failed to find spv file at {:?}", shader_path));

    spv_file
        .bytes()
        .filter_map(|byte| byte.ok())
        .map(|byte| byte as u32)
        .collect()
}
