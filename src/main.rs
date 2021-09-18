use core::panic;
use std::ffi::CString;
use std::os::raw::c_char;
use std::string::FromUtf8Error;

use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::{Surface, Win32Surface};
use ash::vk::{self, ApplicationInfo, InstanceCreateInfo};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

const APP_TITLE: &str = "Rust Renderer VK";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

const VALIDATION_LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];

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

struct HelloTriangleApplication {
    _entry: ash::Entry,
    instance: ash::Instance,
}

impl HelloTriangleApplication {
    pub fn initialize(debug: bool) -> Self {
        let entry = unsafe { ash::Entry::new().unwrap() };
        let instance = HelloTriangleApplication::create_instance(&entry, debug);
        Self {
            _entry: entry,
            instance,
        }
    }

    fn init_window(event_loop: &EventLoop<()>) -> winit::window::Window {
        winit::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build(event_loop)
            .expect("Failed to create window.")
    }

    fn assert_required_layers_available(entry: &ash::Entry) {
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

    fn create_instance(entry: &ash::Entry, debug: bool) -> ash::Instance {
        // Validate extensions
        let windows_required_extensions = vec![
            Surface::name().as_ptr(),
            Win32Surface::name().as_ptr(),
            DebugUtils::name().as_ptr(),
        ];

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
            HelloTriangleApplication::assert_required_layers_available(&entry)
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

        let create_info = if debug {
            println!("Creating instance with the following validation layers:");
            for layer in VALIDATION_LAYERS.iter() {
                println!("\t{}", layer)
            }

            InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_layer_names(&enabled_layers[..])
                .enabled_extension_names(&windows_required_extensions[..])
        } else {
            InstanceCreateInfo::builder()
                .application_info(&app_info)
                .enabled_extension_names(&windows_required_extensions[..])
        };

        // Create instance
        unsafe {
            entry
                .create_instance(&create_info, None)
                .expect("Failed to create Vulkan instance")
        }
    }

    fn main_loop(&mut self, event_loop: EventLoop<()>) {
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
                }
                Event::RedrawRequested(_) => {
                    // Redraw the application.
                    //
                    // It's preferable for applications that do not render continuously to render in
                    // this event rather than in MainEventsCleared, since rendering in here allows
                    // the program to gracefully handle redraws requested by the OS.
                }
                _ => (),
            }
        });
    }

    fn run(&mut self, event_loop: EventLoop<()>) {
        self.main_loop(event_loop);
    }
}

impl Drop for HelloTriangleApplication {
    fn drop(&mut self) {
        unsafe {
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let debug_layers = true;

    let event_loop = EventLoop::new();
    let _window = HelloTriangleApplication::init_window(&event_loop);
    let mut app = HelloTriangleApplication::initialize(debug_layers);
    app.run(event_loop);
}
