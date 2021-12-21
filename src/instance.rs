use std::ffi::CString;

use ash::vk;

use crate::util;

const APP_TITLE: &str = "Rust Renderer VK";

pub fn new<T>(
    entry: &ash::Entry,
    layers: &[CString],
    extensions: &[CString],
    extension_data: &mut [T],
) -> Result<ash::Instance, String>
where
    T: vk::ExtendsInstanceCreateInfo,
{
    let app_name = CString::new(APP_TITLE).unwrap();
    let engine_name = CString::new("Name Pending").unwrap();
    let app_info = vk::ApplicationInfo::builder()
        .application_name(&app_name)
        .application_version(vk::make_api_version(0, 0, 0, 1))
        .engine_name(&engine_name)
        .engine_version(vk::make_api_version(0, 0, 0, 1))
        .api_version(vk::API_VERSION_1_0)
        .build();

    let validation_result = validate_extensions(entry, extensions);
    if validation_result.is_err() {
        let missing = validation_result.unwrap_err();
        return Err(format!(
            "Extensions: {} are unavailable",
            missing.join(", ")
        ));
    };

    let enabled_layers: Vec<*const i8> = layers.iter().map(|l| l.as_ptr()).collect();
    let enabled_extensions: Vec<*const i8> = extensions.iter().map(|e| e.as_ptr()).collect();

    let mut builder = vk::InstanceCreateInfo::builder()
        .application_info(&app_info)
        .enabled_layer_names(&enabled_layers[..])
        .enabled_extension_names(&enabled_extensions);

    for data in extension_data.iter_mut() {
        builder = builder.push_next(data);
    }

    unsafe {
        entry
            .create_instance(&builder, None)
            .map_err(|e| format!("{}", e))
    }
}

fn validate_extensions(entry: &ash::Entry, extensions: &[CString]) -> Result<(), Vec<String>> {
    if extensions.len() == 0 {
        return Ok(());
    };

    let mut missing: Vec<String> = Vec::new();
    if let Ok(available_extension_properties) = entry.enumerate_instance_extension_properties() {
        for required_extension in extensions.iter().map(|e| String::from(e.to_str().unwrap())) {
            let is_present = available_extension_properties
                .iter()
                .map(|p| util::read_vk_string(&p.extension_name).unwrap())
                .fold(false, |acc, current| acc || current.eq(&required_extension));

            if !is_present {
                missing.push(required_extension.clone())
            }
        }
    };

    if missing.len() > 0 {
        Err(missing)
    } else {
        Ok(())
    }
}
