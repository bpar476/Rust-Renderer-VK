use std::ffi::CString;

use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::{Surface, Win32Surface};
use ash::vk::{self, ApplicationInfo, InstanceCreateInfo};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

const APP_TITLE: &str = "Rust Renderer VK";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

struct HelloTriangleApplication {
    _entry: ash::Entry,
    instance: ash::Instance,
}

impl HelloTriangleApplication {
    pub fn initialize() -> Self {
        let entry = unsafe { ash::Entry::new().unwrap() };
        let instance = HelloTriangleApplication::create_instance(&entry);
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

    fn create_instance(entry: &ash::Entry) -> ash::Instance {
        let windows_required_extensions = vec![
            Surface::name().as_ptr(),
            Win32Surface::name().as_ptr(),
            DebugUtils::name().as_ptr(),
        ];

        // Check available extensions
        if let Ok(properties) = entry.enumerate_instance_extension_properties() {
            properties
                .iter()
                .map(|property| {
                    property
                        .extension_name
                        .iter()
                        .map(|char| *char as u8)
                        .collect()
                })
                .map(|name_raw| unsafe { CString::from_vec_unchecked(name_raw) })
                .for_each(|name| println!("Loaded Vulkan extension [{}]", name.to_str().unwrap()));
        };

        let app_name = CString::new(APP_TITLE).unwrap();
        let engine_name = CString::new("Name Pending").unwrap();
        let app_info = ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 0, 0, 1))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 0, 0, 1))
            .api_version(vk::API_VERSION_1_0)
            .build();

        let create_info = InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(&windows_required_extensions[..]);

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
    let event_loop = EventLoop::new();
    let _window = HelloTriangleApplication::init_window(&event_loop);
    let mut app = HelloTriangleApplication::initialize();
    app.run(event_loop);
}
