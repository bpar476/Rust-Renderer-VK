use std::ffi;

use ash::{extensions::ext, vk};

use crate::{instance, util};

const VALIDATION_LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];

pub type DebugMessengerSignature = unsafe extern "system" fn(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_types: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    p_user_data: *mut ffi::c_void,
) -> vk::Bool32;

pub struct Configuration {
    _severities: vk::DebugUtilsMessageSeverityFlagsEXT,
    _callback: DebugMessengerSignature,
    _loader: Option<ext::DebugUtils>,
    _messenger: Option<vk::DebugUtilsMessengerEXT>,
}

impl Configuration {
    pub fn new(
        severities: vk::DebugUtilsMessageSeverityFlagsEXT,
        callback: DebugMessengerSignature,
    ) -> Self {
        Self {
            _severities: severities,
            _callback: callback,
            _loader: None,
            _messenger: None,
        }
    }

    /// If the result is OK, it will contain the layers that should be loaded for debug mode
    /// The given entry is used to validate that the given layers are available on the device
    /// The result will be an error with a message if any required layers are not present.
    pub fn instance_validation_layers(
        &self,
        entry: &ash::Entry,
    ) -> Result<Vec<ffi::CString>, String> {
        match entry.enumerate_instance_layer_properties() {
            Ok(layers) => {
                let mut missing: Vec<String> = Vec::new();
                for validation_layer in VALIDATION_LAYERS.iter() {
                    let is_present = layers
                        .iter()
                        .map(|layer| util::read_vk_string(&layer.layer_name).unwrap())
                        .fold(false, |acc, current| acc || current.eq(validation_layer));

                    if !is_present {
                        missing.push(String::from(validation_layer.clone()));
                    }
                }

                if missing.len() > 0 {
                    Err(format!("Missing extensions: {}", missing.join(", ")))
                } else {
                    Ok(VALIDATION_LAYERS
                        .iter()
                        .map(|layer| ffi::CString::new(layer.clone()).unwrap())
                        .collect())
                }
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Returns the names of all extensions that should be loaded for debug mode
    /// It does no validation that the extensions are available on the device.
    pub fn messenger_extension(&self) -> instance::Extension<vk::DebugUtilsMessengerCreateInfoEXT> {
        let name = ext::DebugUtils::name().to_owned();
        let ci = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(self._severities)
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
            .pfn_user_callback(Some(self._callback))
            .build();

        instance::Extension { name, data: ci }
    }

    /// Registers the debug messenger callback with the given vulkan instance.
    pub fn create_messenger(
        &mut self,
        entry: &ash::Entry,
        instance: &ash::Instance,
    ) -> Result<vk::DebugUtilsMessengerEXT, String> {
        match self._loader {
            Some(_) => Err(String::from(
                "Messenger already configured for a vulkan instance",
            )),
            None => {
                let loader = ash::extensions::ext::DebugUtils::new(&entry, &instance);

                let create_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
                    .message_severity(self._severities)
                    .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
                    .pfn_user_callback(Some(self._callback));

                unsafe {
                    match loader.create_debug_utils_messenger(&create_info, None) {
                        Err(result) => Err(format!("{}", result)),
                        Ok(messenger) => {
                            self._loader = Some(loader);
                            self._messenger = Some(messenger);
                            Ok(messenger)
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Configuration {
    fn drop(&mut self) {
        if let (Some(loader), Some(messenger)) = (&self._loader, self._messenger) {
            unsafe { loader.destroy_debug_utils_messenger(messenger, None) };
        }
    }
}
