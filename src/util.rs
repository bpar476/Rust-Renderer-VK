use std::{fs::File, io::Read, path::Path};

pub fn read_shader_code(shader_path: &Path) -> Vec<u8> {
    let spv_file =
        File::open(shader_path).expect(&format!("Failed to find spv file at {:?}", shader_path));
    let shader_code: Vec<u8> = spv_file.bytes().filter_map(|byte| byte.ok()).collect();

    shader_code
}
