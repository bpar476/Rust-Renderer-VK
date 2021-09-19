use core::panic;
use std::ffi::{c_void, CStr, CString};
use std::ops::{BitAndAssign, Not};
use std::os::raw::c_char;
use std::string::FromUtf8Error;

use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::{Surface, Win32Surface};
use ash::vk::{self, ApplicationInfo, InstanceCreateInfo};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

const APP_TITLE: &str = "Rust Renderer VK";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

const VALIDATION_LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];

// Debug utils callback
unsafe extern "system" fn vulkan_debug_utils_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _p_user_data: *mut c_void,
) -> vk::Bool32 {
    let severity = match message_severity {
        vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE => "VERBOSE",
        vk::DebugUtilsMessageSeverityFlagsEXT::INFO => "INFO",
        vk::DebugUtilsMessageSeverityFlagsEXT::WARNING => "WARN",
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR => "ERROR",
        _ => "???",
    };
    let kind = match message_type {
        vk::DebugUtilsMessageTypeFlagsEXT::GENERAL => "general",
        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION => "validation",
        vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE => "perf",
        _ => "???",
    };

    let message = CStr::from_ptr((*p_callback_data).p_message);
    eprintln!("[VK DEBUG][{}][{}]: {:?}", severity, kind, message);

    // Return false to indicate that validation should not cause a crash
    vk::FALSE
}

fn read_vk_string(chars: &[c_char]) -> Result<String, FromUtf8Error> {
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

struct QueueFamilyIndices {
    graphics_family: Option<u32>,
}

impl QueueFamilyIndices {
    pub fn is_complete(&self) -> bool {
        self.graphics_family.is_some()
    }
}

struct HelloTriangleApplication {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical_device: ash::vk::PhysicalDevice,
    queue_families: QueueFamilyIndices,

    debug_loader: Option<ash::extensions::ext::DebugUtils>,
    debug_messenger_ext: Option<vk::DebugUtilsMessengerEXT>,
}

impl HelloTriangleApplication {
    pub fn initialize(debug: bool) -> Self {
        let entry = unsafe { ash::Entry::new().unwrap() };
        let instance = HelloTriangleApplication::create_instance(&entry, debug);

        let (debug_loader, debug_messenger_ext) = if debug {
            let (loader, messenger_ext) =
                HelloTriangleApplication::create_debug_messenger(&entry, &instance);
            (Some(loader), Some(messenger_ext))
        } else {
            (None, None)
        };

        let physical_device = match HelloTriangleApplication::pick_physical_device(&instance) {
            Some(device) => device,
            None => panic!("No suitable physical device"),
        };

        let queue_families =
            HelloTriangleApplication::find_queue_families(&instance, &physical_device);

        Self {
            _entry: entry,
            instance,
            debug_loader,
            debug_messenger_ext,
            physical_device,
            queue_families,
        }
    }

    /**
    Instance creation
    */
    fn create_instance(entry: &ash::Entry, debug: bool) -> ash::Instance {
        let extensions = HelloTriangleApplication::get_extensions(debug);

        // Check available extensions
        if let Ok(properties) = entry.enumerate_instance_extension_properties() {
            // TODO check that required extensions are present
            properties
                .iter()
                .map(|property| read_vk_string(&property.extension_name[..]).unwrap())
                .for_each(|name| println!("Loaded Vulkan extension: {}", name));
        } else {
            panic!("Unable to load required platform extensions")
        };

        // Validate validation layers
        if debug {
            HelloTriangleApplication::assert_required_validation_layers_available(&entry)
        };

        let required_validation_layer_raw_names: Vec<CString> = VALIDATION_LAYERS
            .iter()
            .map(|layer_name| CString::new(*layer_name).unwrap())
            .collect();
        let enabled_layers: Vec<*const i8> = required_validation_layer_raw_names
            .iter()
            .map(|layer_name| layer_name.as_ptr())
            .collect();

        // Create initialisation structs
        let app_name = CString::new(APP_TITLE).unwrap();
        let engine_name = CString::new("Name Pending").unwrap();
        let app_info = ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 0, 0, 1))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 0, 0, 1))
            .api_version(vk::API_VERSION_1_0)
            .build();

        let mut debug_utils_create_info =
            HelloTriangleApplication::build_debug_utils_messenger_create_info();
        let create_info = if debug {
            println!("Creating instance with the following validation layers:");
            for layer in VALIDATION_LAYERS.iter() {
                println!("\t{}", layer)
            }

            InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_layer_names(&enabled_layers[..])
                .enabled_extension_names(&extensions[..])
                .push_next(&mut debug_utils_create_info)
        } else {
            InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_extension_names(&extensions[..])
        };

        // Create instance
        unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Failed to create Vulkan instance")
        }
    }

    fn get_extensions(debug: bool) -> Vec<*const c_char> {
        let mut extensions: Vec<*const c_char> = vec![];

        extensions.push(Surface::name().as_ptr());
        extensions.push(Win32Surface::name().as_ptr());

        if debug {
            extensions.push(DebugUtils::name().as_ptr());
        }
        extensions
    }

    fn assert_required_validation_layers_available(entry: &ash::Entry) {
        match entry.enumerate_instance_layer_properties() {
            Ok(layers) => {
                let layer_names: Vec<String> = layers
                    .iter()
                    .map(|layer| read_vk_string(&layer.layer_name[..]).unwrap())
                    .collect();

                layer_names
                    .iter()
                    .for_each(|layer| println!("Found validation layer {}", layer));

                let mut unavailable_layers: Vec<&str> = vec![];
                for layer in VALIDATION_LAYERS.iter() {
                    if !layer_names.iter().any(|layer_name| layer_name.eq(layer)) {
                        unavailable_layers.push(layer)
                    }
                }

                if unavailable_layers.len() > 0 {
                    unavailable_layers.iter().for_each(|&layer| {
                        println!("Required validation layer {} is not available", layer)
                    });

                    panic!("Could not find required validation layers. See log for details.")
                }
            }
            _ => panic!("Unable to load Vulkan validation layers"),
        }
    }

    fn build_debug_utils_messenger_create_info() -> vk::DebugUtilsMessengerCreateInfoEXT {
        let mut severities = vk::DebugUtilsMessageSeverityFlagsEXT::all();
        severities.bitand_assign(vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE.not());

        vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(severities)
            .message_type(vk::DebugUtilsMessageTypeFlagsEXT::all())
            .pfn_user_callback(Some(vulkan_debug_utils_callback))
            .build()
    }

    /**
    Debug Utils validation layer
    */
    fn create_debug_messenger(
        entry: &ash::Entry,
        instance: &ash::Instance,
    ) -> (ash::extensions::ext::DebugUtils, vk::DebugUtilsMessengerEXT) {
        let create_info = HelloTriangleApplication::build_debug_utils_messenger_create_info();
        // This DebugUtils struct loads the extension function for us since debug utils are not a part of the standard
        // they are not loaded when creating the Entry
        let debug_utils_loader = ash::extensions::ext::DebugUtils::new(entry, instance);

        let messenger = unsafe {
            debug_utils_loader
                .create_debug_utils_messenger(&create_info, None)
                .expect("Debug Utils Callback")
        };

        (debug_utils_loader, messenger)
    }

    /**
    Physical Device
    */
    fn pick_physical_device(instance: &ash::Instance) -> Option<vk::PhysicalDevice> {
        let devices = unsafe { instance.enumerate_physical_devices() };

        match devices {
            Ok(devices) => {
                if devices.len() == 0 {
                    None
                } else {
                    println!("Found {} devices", devices.len());
                    // TODO confirm device name in use
                    if let Some(device) = devices.iter().find(|&device| {
                        HelloTriangleApplication::is_device_suitable(instance, device)
                    }) {
                        Some(*device)
                    } else {
                        None
                    }
                }
            }
            Err(_) => None,
        }
    }

    fn is_device_suitable(instance: &ash::Instance, device: &vk::PhysicalDevice) -> bool {
        let properties = unsafe { instance.get_physical_device_properties(*device) };
        let features = unsafe { instance.get_physical_device_features(*device) };

        println!(
            "Evaluating suitability of device [{}]",
            read_vk_string(&properties.device_name[..]).unwrap()
        );

        let supports_required_families =
            HelloTriangleApplication::find_queue_families(instance, device).is_complete();

        properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
            && features.geometry_shader == 1
            && supports_required_families
    }

    /**
    Queue Families
    */
    fn find_queue_families(
        instance: &ash::Instance,
        device: &vk::PhysicalDevice,
    ) -> QueueFamilyIndices {
        let mut indices = QueueFamilyIndices {
            graphics_family: None,
        };

        let properties = unsafe { instance.get_physical_device_queue_family_properties(*device) };

        for (i, family) in properties.iter().enumerate() {
            if family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                indices.graphics_family = Some(i as u32);
            }

            if indices.is_complete() {
                break;
            }
        }

        indices
    }

    /**
    Main loop
    */
    fn init_window(event_loop: &EventLoop<()>) -> winit::window::Window {
        winit::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build(event_loop)
            .expect("Failed to create window.")
    }

    fn draw_frame(&mut self) {
        // Drawing will be here
    }

    fn main_loop(mut self, event_loop: EventLoop<()>, window: Window) {
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    println!("The close button was pressed; stopping");
                    *control_flow = ControlFlow::Exit
                }
                Event::MainEventsCleared => {
                    // Application update code.
                    // Queue a RedrawRequested event.
                    //
                    // You only need to call this if you've determined that you need to redraw, in
                    // applications which do not always need to. Applications that redraw continuously
                    // can just render here instead.

                    window.request_redraw()
                }
                Event::RedrawRequested(_) => {
                    // Redraw the application.
                    //
                    // It's preferable for applications that do not render continuously to render in
                    // this event rather than in MainEventsCleared, since rendering in here allows
                    // the program to gracefully handle redraws requested by the OS.

                    // NOTE: This function does nothing, however if we don't reference `self` in this loop,
                    // Drop will never be called for our application.
                    self.draw_frame();
                }
                _ => (),
            }
        });
    }

    fn run(self, event_loop: EventLoop<()>, window: Window) {
        self.main_loop(event_loop, window);
    }
}

impl Drop for HelloTriangleApplication {
    fn drop(&mut self) {
        unsafe {
            if let (Some(loader), Some(messenger)) =
                // FIXME: Not quite sure why this needs to be a ref
                (self.debug_loader.as_ref(), self.debug_messenger_ext)
            {
                loader.destroy_debug_utils_messenger(messenger, None)
            }
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let debug_layers = true;

    let event_loop = EventLoop::new();
    let window = HelloTriangleApplication::init_window(&event_loop);
    let app = HelloTriangleApplication::initialize(debug_layers);
    app.run(event_loop, window);
}
