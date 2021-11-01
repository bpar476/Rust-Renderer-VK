use core::panic;
use num;
use std::ffi::{c_void, CStr, CString};
use std::ops::{BitAndAssign, Deref, Not};
use std::os::raw::c_char;
use std::path::Path;
use std::string::FromUtf8Error;
mod util;

use ash::extensions::ext::DebugUtils;
use ash::extensions::khr::{Surface, Swapchain, Win32Surface};
use ash::vk::{
    self, ApplicationInfo, DeviceQueueCreateInfo, InstanceCreateInfo, SurfaceCapabilitiesKHR,
};
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
    present_family: Option<u32>,
}

impl QueueFamilyIndices {
    pub fn is_complete(&self) -> bool {
        self.graphics_family.is_some() && self.present_family.is_some()
    }
}

struct SwapChainSupportDetails {
    capabilities: ash::vk::SurfaceCapabilitiesKHR,
    formats: Vec<ash::vk::SurfaceFormatKHR>,
    present_modes: Vec<ash::vk::PresentModeKHR>,
}

struct SwapChainData {
    loader: ash::extensions::khr::Swapchain,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    format: vk::Format,
    extent: vk::Extent2D,
}

struct HelloTriangleApplication {
    _entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: ash::extensions::khr::Surface,
    physical_device: ash::vk::PhysicalDevice,
    queue_families: QueueFamilyIndices,
    logical_device: ash::Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    debug_loader: Option<ash::extensions::ext::DebugUtils>,
    debug_messenger_ext: Option<vk::DebugUtilsMessengerEXT>,

    swapchain_data: SwapChainData,
    swapchain_image_views: Vec<vk::ImageView>,
}

impl HelloTriangleApplication {
    pub fn initialize(window: &winit::window::Window, debug: bool) -> Self {
        let entry = unsafe { ash::Entry::new().unwrap() };
        let instance = HelloTriangleApplication::create_instance(&entry, debug);

        let (debug_loader, debug_messenger_ext) = if debug {
            let (loader, messenger_ext) =
                HelloTriangleApplication::create_debug_messenger(&entry, &instance);
            (Some(loader), Some(messenger_ext))
        } else {
            (None, None)
        };

        // We need a handle to the surface loader so we can call the extension functions
        let (surface_loader, surface) =
            HelloTriangleApplication::create_win32_surface(&entry, &instance, window);

        let physical_device = match HelloTriangleApplication::pick_physical_device(
            &instance,
            &surface_loader,
            &surface,
        ) {
            Some(device) => device,
            None => panic!("No suitable physical device"),
        };

        let queue_families = HelloTriangleApplication::find_queue_families(
            &instance,
            &physical_device,
            &surface_loader,
            &surface,
        );

        let logical_device = HelloTriangleApplication::create_logical_device(
            &instance,
            &physical_device,
            &queue_families,
            debug,
        );

        let graphics_queue = HelloTriangleApplication::get_device_queue(
            &logical_device,
            queue_families
                .graphics_family
                .expect("Graphics queue family index"),
        );
        let present_queue = HelloTriangleApplication::get_device_queue(
            &logical_device,
            queue_families
                .present_family
                .expect("Present queue family index"),
        );

        let swapchain_data = HelloTriangleApplication::create_swap_chain(
            &instance,
            &logical_device,
            &surface_loader,
            &physical_device,
            &surface,
            window,
            &queue_families,
        );

        let swapchain_image_views =
            HelloTriangleApplication::create_image_views(&logical_device, &swapchain_data);

        let graphics_pipeline = HelloTriangleApplication::create_graphics_pipeline();

        Self {
            _entry: entry,
            instance,
            debug_loader,
            debug_messenger_ext,
            surface,
            surface_loader,
            physical_device,
            queue_families,
            logical_device,
            graphics_queue,
            present_queue,
            swapchain_data,
            swapchain_image_views,
        }
    }

    /**
    Instance creation
    */
    fn create_instance(entry: &ash::Entry, debug: bool) -> ash::Instance {
        let extensions = HelloTriangleApplication::get_instance_extensions(debug);

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
        let validation_layers: Vec<*const c_char> = required_validation_layer_raw_names
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
                .enabled_layer_names(&validation_layers[..])
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

    fn get_instance_extensions(debug: bool) -> Vec<*const c_char> {
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
    fn pick_physical_device(
        instance: &ash::Instance,
        surface_loader: &ash::extensions::khr::Surface,
        surface: &vk::SurfaceKHR,
    ) -> Option<vk::PhysicalDevice> {
        let devices = unsafe { instance.enumerate_physical_devices() };

        match devices {
            Ok(devices) => {
                if devices.len() == 0 {
                    None
                } else {
                    println!("Found {} devices", devices.len());
                    // TODO confirm device name in use
                    if let Some(device) = devices.iter().find(|&device| {
                        HelloTriangleApplication::is_device_suitable(
                            instance,
                            device,
                            surface_loader,
                            surface,
                        )
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

    fn is_device_suitable(
        instance: &ash::Instance,
        device: &vk::PhysicalDevice,
        surface_loader: &ash::extensions::khr::Surface,
        surface: &vk::SurfaceKHR,
    ) -> bool {
        let properties = unsafe { instance.get_physical_device_properties(*device) };
        let features = unsafe { instance.get_physical_device_features(*device) };

        let required_device_extensions: Vec<String> =
            HelloTriangleApplication::get_device_extensions()
                .iter()
                .map(|&name| String::from(name.to_str().expect("Swapchain extension name")))
                .collect();
        let required_device_extensions_supported =
            HelloTriangleApplication::check_device_extension_support(
                &instance,
                device,
                required_device_extensions,
            );

        println!(
            "Evaluating suitability of device [{}]",
            read_vk_string(&properties.device_name[..]).unwrap()
        );

        let supports_required_families = HelloTriangleApplication::find_queue_families(
            instance,
            device,
            surface_loader,
            surface,
        )
        .is_complete();

        if required_device_extensions_supported {
            // Only check swap chain support if the swap chain device extensions are supported
            let swap_chain_support = unsafe {
                HelloTriangleApplication::query_swap_chain_support(surface_loader, device, surface)
            };
            let swap_chain_adequate = !swap_chain_support.formats.is_empty()
                && !swap_chain_support.present_modes.is_empty();

            properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                && features.geometry_shader == 1
                && supports_required_families
                && swap_chain_adequate
        } else {
            false
        }
    }

    fn check_device_extension_support(
        instance: &ash::Instance,
        device: &vk::PhysicalDevice,
        required_extensions: Vec<String>,
    ) -> bool {
        // TODO why doesn't dereferencing move device
        let available_extensions: Vec<String> =
            unsafe { instance.enumerate_device_extension_properties(*device) }
                .expect("Reading device extensions")
                .iter()
                .map(|extension| {
                    read_vk_string(&extension.extension_name[..])
                        .expect("Reading device extension name")
                })
                .collect();

        println!("Found {:?} device extensions", available_extensions);

        let mut all_extensions_present = true;
        for required_extension in required_extensions.iter() {
            all_extensions_present =
                available_extensions.contains(required_extension) && all_extensions_present
        }
        // TODO print missing extensions

        all_extensions_present
    }

    /**
    Queue Families
    */
    fn find_queue_families(
        instance: &ash::Instance,
        device: &vk::PhysicalDevice,
        surface_loader: &ash::extensions::khr::Surface,
        surface: &vk::SurfaceKHR,
    ) -> QueueFamilyIndices {
        let mut indices = QueueFamilyIndices {
            graphics_family: None,
            present_family: None,
        };

        let properties = unsafe { instance.get_physical_device_queue_family_properties(*device) };

        for (i, family) in properties.iter().enumerate() {
            if family.queue_count > 0 && family.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                indices.graphics_family = Some(i as u32);
            }

            let is_present_support = unsafe {
                surface_loader
                    .get_physical_device_surface_support(*device, i as u32, *surface)
                    .expect("Get physical device surface support")
            };

            if family.queue_count > 0 && is_present_support {
                indices.present_family = Some(i as u32)
            }

            if indices.is_complete() {
                break;
            }
        }

        indices
    }

    /**
     * Logical device
     */
    fn get_device_extensions() -> Vec<&'static CStr> {
        vec![ash::extensions::khr::Swapchain::name()]
    }

    fn create_logical_device(
        instance: &ash::Instance,
        physical_device: &vk::PhysicalDevice,
        queue_indices: &QueueFamilyIndices,
        debug: bool,
    ) -> ash::Device {
        let mut queue_create_infos: Vec<DeviceQueueCreateInfo> = vec![];

        // Use a set to remove duplicate queue indices. It is illegal to request a queue created with the same queue index multiple times
        use std::collections::HashSet;
        let mut unique_queue_families = HashSet::new();
        unique_queue_families.insert(queue_indices.graphics_family.unwrap());
        unique_queue_families.insert(queue_indices.present_family.unwrap());

        for index in unique_queue_families.iter() {
            queue_create_infos.push(
                vk::DeviceQueueCreateInfo::builder()
                    .queue_family_index(*index)
                    .queue_priorities(&[1.0])
                    .build(),
            )
        }
        let device_features = vk::PhysicalDeviceFeatures::builder().build();

        let create_infos = &queue_create_infos[..];
        let required_validation_layer_raw_names: Vec<CString> = VALIDATION_LAYERS
            .iter()
            .map(|layer_name| CString::new(*layer_name).unwrap())
            .collect();
        let validation_layers: Vec<*const c_char> = required_validation_layer_raw_names
            .iter()
            .map(|layer_name| layer_name.as_ptr())
            .collect();
        let enabled_extension_names: Vec<*const c_char> =
            HelloTriangleApplication::get_device_extensions()
                .iter()
                .map(|&name| name.as_ptr())
                .collect();
        let device_create_info = if debug {
            vk::DeviceCreateInfo::builder()
                .queue_create_infos(create_infos)
                .enabled_features(&device_features)
                .enabled_layer_names(&validation_layers[..])
                .enabled_extension_names(&enabled_extension_names[..])
        } else {
            vk::DeviceCreateInfo::builder()
                .queue_create_infos(create_infos)
                .enabled_features(&device_features)
        };

        unsafe {
            match instance.create_device(*physical_device, &device_create_info, None) {
                Ok(device) => device,
                _ => panic!("Logical device creation"),
            }
        }
    }

    /**
     * Queues
     */
    fn get_device_queue(logical_device: &ash::Device, index: u32) -> vk::Queue {
        unsafe { logical_device.get_device_queue(index, 0) }
    }

    /**
     * Presentation
     */
    fn create_win32_surface(
        entry: &ash::Entry,
        instance: &ash::Instance,
        window: &winit::window::Window,
    ) -> (ash::extensions::khr::Surface, vk::SurfaceKHR) {
        use std::ptr;
        use winapi::shared::windef::HWND;
        use winapi::um::libloaderapi::GetModuleHandleW;
        use winit::platform::windows::WindowExtWindows;

        let hwnd = window.hwnd() as HWND;
        let hinstance = unsafe { GetModuleHandleW(ptr::null()) as *const c_void };
        let win32_create_info = vk::Win32SurfaceCreateInfoKHR::builder()
            .hinstance(hinstance)
            .hwnd(hwnd as *const c_void);
        let win32_surface_loader = Win32Surface::new(entry, instance);
        let surface = unsafe {
            win32_surface_loader
                .create_win32_surface(&win32_create_info, None)
                .expect("Win32 Surface")
        };
        let surface_loader = ash::extensions::khr::Surface::new(entry, instance);
        (surface_loader, surface)
    }

    /**
     * Swap chain
     */
    unsafe fn query_swap_chain_support(
        surface_loader: &ash::extensions::khr::Surface,
        device: &ash::vk::PhysicalDevice,
        surface: &ash::vk::SurfaceKHR,
    ) -> SwapChainSupportDetails {
        let capabilities = surface_loader
            .get_physical_device_surface_capabilities(*device, *surface)
            .expect("Physical device surface capabilities");

        let formats = surface_loader
            .get_physical_device_surface_formats(*device, *surface)
            .expect("Surface formats");
        let present_modes = surface_loader
            .get_physical_device_surface_present_modes(*device, *surface)
            .expect("Present Modes");

        SwapChainSupportDetails {
            capabilities,
            formats,
            present_modes,
        }
    }

    fn choose_swap_surface_format(
        available_formats: &Vec<ash::vk::SurfaceFormatKHR>,
    ) -> ash::vk::SurfaceFormatKHR {
        available_formats
            .iter()
            .filter(|&format| {
                format.format == ash::vk::Format::B8G8R8A8_SRGB
                    && format.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .next()
            .unwrap_or(&available_formats[0])
            .to_owned()
    }

    fn choose_swap_present_mode(available_modes: &Vec<vk::PresentModeKHR>) -> vk::PresentModeKHR {
        if available_modes.contains(&vk::PresentModeKHR::MAILBOX) {
            vk::PresentModeKHR::MAILBOX
        } else {
            // FIFO is guaranteed to be available if device supports presentation
            vk::PresentModeKHR::FIFO
        }
    }

    fn choose_swap_extent(
        capabilities: &vk::SurfaceCapabilitiesKHR,
        window: &winit::window::Window,
    ) -> vk::Extent2D {
        if capabilities.current_extent.width != u32::MAX {
            // The window manager has set the extent for us
            // https://khronos.org/registry/vulkan/specs/1.2-extensions/man/html/VkSurfaceCapabilitiesKHR.html
            capabilities.current_extent
        } else {
            let size = window.inner_size();
            let min = capabilities.min_image_extent;
            let max = capabilities.max_image_extent;
            vk::Extent2D::builder()
                .width(num::clamp(size.width, min.width, max.width))
                .height(num::clamp(size.height, min.height, max.height))
                .build()
        }
    }

    fn create_swap_chain(
        instance: &ash::Instance,
        logical_device: &ash::Device,
        surface_loader: &ash::extensions::khr::Surface,
        physical_device: &ash::vk::PhysicalDevice,
        surface: &vk::SurfaceKHR,
        window: &winit::window::Window,
        indicies: &QueueFamilyIndices,
    ) -> SwapChainData {
        let swap_chain_support = unsafe {
            HelloTriangleApplication::query_swap_chain_support(
                surface_loader,
                physical_device,
                surface,
            )
        };
        let format =
            HelloTriangleApplication::choose_swap_surface_format(&swap_chain_support.formats);
        let present_mode =
            HelloTriangleApplication::choose_swap_present_mode(&swap_chain_support.present_modes);
        let extent =
            HelloTriangleApplication::choose_swap_extent(&swap_chain_support.capabilities, window);

        // Minimum images plus one so we always have an image to draw to while driver is working
        let preferred_image_count = swap_chain_support.capabilities.min_image_count + 1;
        // If max image count is 0 it means there is no max image count
        let image_count = if swap_chain_support.capabilities.max_image_count > 0
            && swap_chain_support.capabilities.max_image_count < preferred_image_count
        {
            swap_chain_support.capabilities.max_image_count
        } else {
            preferred_image_count
        };

        let (image_sharing_mode, families) = if indicies.graphics_family != indicies.present_family
        {
            // Both the graphics and the present family need to access swap chain images. If these queue families are not the
            // same queue, then use concurent sharing mode. This is worse performance but allows us to share images without
            // explicitly managing image ownership.
            (
                vk::SharingMode::CONCURRENT,
                vec![
                    indicies.graphics_family.unwrap(),
                    indicies.present_family.unwrap(),
                ],
            )
        } else {
            // If the queue families are the same queue then the queue has exclusive use of swap chain images so we don't need to
            // manage ownership anyway
            (vk::SharingMode::EXCLUSIVE, vec![])
        };

        // See https://www.khronos.org/registry/vulkan/specs/1.2-extensions/man/html/VkSwapchainCreateInfoKHR.html for reference on all options
        let create_info = vk::SwapchainCreateInfoKHR::builder()
            .surface(*surface)
            .min_image_count(image_count)
            .image_format(format.format)
            .image_color_space(format.color_space)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .pre_transform(swap_chain_support.capabilities.current_transform)
            // Alpha blending between other windows in window system
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(present_mode)
            .clipped(true)
            .image_sharing_mode(image_sharing_mode)
            .queue_family_indices(&families[..]);

        let swapchain_loader = ash::extensions::khr::Swapchain::new(instance, logical_device);
        let swapchain =
            unsafe { swapchain_loader.create_swapchain(&create_info, None) }.expect("Swapchain");

        let images =
            unsafe { swapchain_loader.get_swapchain_images(swapchain) }.expect("Swapchain images");

        SwapChainData {
            loader: swapchain_loader,
            swapchain: swapchain,
            format: format.format,
            extent: extent,
            images,
        }
    }

    fn create_image_views(
        device: &ash::Device,
        swapchain_data: &SwapChainData,
    ) -> Vec<vk::ImageView> {
        swapchain_data
            .images
            .iter()
            .map(|image| {
                let ci = vk::ImageViewCreateInfo::builder()
                    // TODO is copying bad here?
                    .image(*image)
                    .view_type(vk::ImageViewType::TYPE_2D)
                    .format(swapchain_data.format)
                    .components(
                        vk::ComponentMapping::builder()
                            .r(vk::ComponentSwizzle::IDENTITY)
                            .g(vk::ComponentSwizzle::IDENTITY)
                            .b(vk::ComponentSwizzle::IDENTITY)
                            .a(vk::ComponentSwizzle::IDENTITY)
                            .build(),
                    )
                    // TODO look up what a subresource range is from khronos reference
                    .subresource_range(
                        vk::ImageSubresourceRange::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .base_mip_level(0)
                            .level_count(1)
                            .base_array_layer(0)
                            .layer_count(1)
                            .build(),
                    );
                unsafe {
                    device
                        .create_image_view(&ci, None)
                        .expect("Creating image view")
                }
            })
            .collect()
    }

    fn create_graphics_pipeline(device: &ash::Device) {
        let vert_shader_code =
            util::read_shader_code(Path::new(env!("OUT_DIR")).join("vert.spv").as_path());
        let frag_shader_code =
            util::read_shader_code(Path::new(env!("OUT_DIR")).join("frag.spv").as_path());

        let vert_shader_module =
            HelloTriangleApplication::create_shader_module(device, vert_shader_code);
        let frag_shader_module =
            HelloTriangleApplication::create_shader_module(device, frag_shader_code);

        let main_fn_name = CString::new("main").unwrap();
        let vert_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_shader_module)
            .name(main_fn_name.as_c_str());
        let frag_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_shader_module)
            .name(main_fn_name.as_c_str());
        let shader_stages = vec![vert_stage_builder, frag_stage_builder];

        unsafe { device.destroy_shader_module(vert_shader_module, None) };
        unsafe { device.destroy_shader_module(frag_shader_module, None) };
    }

    fn create_shader_module(device: &ash::Device, code: Vec<u32>) -> vk::ShaderModule {
        let builder = vk::ShaderModuleCreateInfo::builder().code(&code[..]);
        unsafe {
            device
                .create_shader_module(&builder, None)
                .expect("Shader module")
        }
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
            for image_view in self.swapchain_image_views.iter() {
                self.logical_device.destroy_image_view(*image_view, None)
            }
            self.swapchain_data
                .loader
                .destroy_swapchain(self.swapchain_data.swapchain, None);
            self.surface_loader.destroy_surface(self.surface, None);
            self.logical_device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let debug_layers = true;

    let event_loop = EventLoop::new();
    let window = HelloTriangleApplication::init_window(&event_loop);
    let app = HelloTriangleApplication::initialize(&window, debug_layers);
    app.run(event_loop, window);
}
