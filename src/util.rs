use std::{ffi, fs, os::raw, path, string};

pub fn read_shader_code(shader_path: &path::Path) -> Vec<u32> {
    let mut spv_file = fs::File::open(shader_path)
        .expect(&format!("Failed to find spv file at {:?}", shader_path));

    ash::util::read_spv(&mut spv_file).expect("spv file")
}

pub fn read_vk_string(chars: &[raw::c_char]) -> Result<String, string::FromUtf8Error> {
    let terminator = '\0' as u8;
    let mut content: Vec<u8> = vec![];

    for raw in chars.iter() {
        let ch = (*raw) as u8;

        if ch != terminator {
            content.push(ch);
        } else {
            break;
        }
    }

    String::from_utf8(content)
}
