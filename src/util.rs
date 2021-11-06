use std::{fs::File, io::Read, path::Path};

pub fn read_shader_code(shader_path: &Path) -> Vec<u32> {
    let mut spv_file =
        File::open(shader_path).expect(&format!("Failed to find spv file at {:?}", shader_path));

    ash::util::read_spv(&mut spv_file).expect("spv file")
}
