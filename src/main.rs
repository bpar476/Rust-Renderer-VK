use cgmath::{Deg, Euler, Matrix4, Point3, Rad, Vector3};
use core::panic;
use memoffset::offset_of;
use num::{self, range};
use std::convert::TryInto;
use std::ffi::{c_void, CStr, CString};
use std::mem::{self, size_of};
use std::ops::{BitAndAssign, BitOr, BitOrAssign, Deref, Not};
use std::os::raw::c_char;
use std::path::Path;
use std::time::Instant;
mod debug;
mod instance;
mod util;

use ash::extensions::khr::{Surface, Win32Surface};
use ash::vk::{self, DeviceQueueCreateInfo, MemoryMapFlags};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

const APP_TITLE: &str = "Rust Renderer VK";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

const VALIDATION_LAYERS: [&str; 1] = ["VK_LAYER_KHRONOS_validation"];
const MAX_FRAMES_IN_FLIGHT: usize = 2;

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

#[repr(C)]
#[derive(Clone, Debug, Copy)]
struct UniformBufferObject {
    model: Matrix4<f32>,
    view: Matrix4<f32>,
    perspective: Matrix4<f32>,
}

struct Vertex {
    pos: [f32; 3],
    color: [f32; 3],
    tex_coord: [f32; 2],
}

impl Vertex {
    fn get_binding_desription() -> vk::VertexInputBindingDescription {
        vk::VertexInputBindingDescription::builder()
            .binding(0)
            .stride(size_of::<Self>() as u32)
            .input_rate(vk::VertexInputRate::VERTEX)
            .build()
    }

    fn get_attribute_descriptions() -> [vk::VertexInputAttributeDescription; 3] {
        let position_binding = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(0)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Self, pos) as u32)
            .build();
        let color_binding = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(1)
            .format(vk::Format::R32G32B32_SFLOAT)
            .offset(offset_of!(Self, color) as u32)
            .build();
        let tex_coord_binding = vk::VertexInputAttributeDescription::builder()
            .binding(0)
            .location(2)
            .format(vk::Format::R32G32_SFLOAT)
            .offset(offset_of!(Self, tex_coord) as u32)
            .build();

        [position_binding, color_binding, tex_coord_binding]
    }
}

const QUAD_VERTICES: [Vertex; 8] = [
    // First quad
    Vertex {
        pos: [-0.5, -0.5, 0.0],
        color: [1.0, 0.0, 0.0],
        tex_coord: [1.0, 0.0],
    },
    Vertex {
        pos: [0.5, -0.5, 0.0],
        color: [0.0, 1.0, 0.0],
        tex_coord: [0.0, 0.0],
    },
    Vertex {
        pos: [0.5, 0.5, 0.0],
        color: [0.0, 0.0, 1.0],
        tex_coord: [0.0, 1.0],
    },
    Vertex {
        pos: [-0.5, 0.5, 0.0],
        color: [1.0, 1.0, 1.0],
        tex_coord: [1.0, 1.0],
    },
    // Second quad
    Vertex {
        pos: [-0.5, -0.5, -0.5],
        color: [1.0, 0.0, 0.0],
        tex_coord: [1.0, 0.0],
    },
    Vertex {
        pos: [0.5, -0.5, -0.5],
        color: [0.0, 1.0, 0.0],
        tex_coord: [0.0, 0.0],
    },
    Vertex {
        pos: [0.5, 0.5, -0.5],
        color: [0.0, 0.0, 1.0],
        tex_coord: [0.0, 1.0],
    },
    Vertex {
        pos: [-0.5, 0.5, -0.5],
        color: [1.0, 1.0, 1.0],
        tex_coord: [1.0, 1.0],
    },
];

const QUAD_INDICES: [u16; 12] = [
    0, 1, 2, 2, 3, 0, // First Quad
    4, 5, 6, 6, 7, 4, // Second Quad
];

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
    window: winit::window::Window,

    _entry: ash::Entry,
    instance: ash::Instance,
    surface: vk::SurfaceKHR,
    surface_loader: ash::extensions::khr::Surface,
    debug_config: Option<debug::Configuration>,
    physical_device: ash::vk::PhysicalDevice,
    physical_device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    queue_families: QueueFamilyIndices,
    logical_device: ash::Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    swapchain_data: SwapChainData,
    swapchain_image_views: Vec<vk::ImageView>,

    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    descriptor_set_layout: vk::DescriptorSetLayout,

    render_pass: vk::RenderPass,
    pipeline_layout: vk::PipelineLayout,
    graphics_pipeline: vk::Pipeline,

    swap_chain_frame_buffers: Vec<vk::Framebuffer>,

    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,

    image_available_semaphores: Vec<vk::Semaphore>,
    render_complete_semaphores: Vec<vk::Semaphore>,
    frame_fences: Vec<vk::Fence>,
    image_fences: Vec<vk::Fence>,

    current_frame: usize,

    frame_buffer_resized: bool,

    vertex_buffer: vk::Buffer,
    vertex_buffer_memory: vk::DeviceMemory,

    index_buffer: vk::Buffer,
    index_buffer_memory: vk::DeviceMemory,

    uniform_buffers: Vec<vk::Buffer>,
    uniform_buffers_memory: Vec<vk::DeviceMemory>,

    start_time: Instant,
    image: vk::Image,
    image_memory: vk::DeviceMemory,
    texture_image_view: vk::ImageView,
    texture_sampler: vk::Sampler,

    depth_image: vk::Image,
    depth_image_memory: vk::DeviceMemory,
    depth_image_view: vk::ImageView,
}

impl HelloTriangleApplication {
    pub fn initialize(
        event_loop: &EventLoop<()>,
        debug_config: Option<debug::Configuration>,
    ) -> Self {
        let window = Self::init_window(&event_loop);

        let mut debug_config = debug_config;
        let entry = unsafe { ash::Entry::new().unwrap() };

        let instance = Self::create_instance(&entry, &debug_config);
        for config in debug_config.iter_mut() {
            let result = config.create_messenger(&entry, &instance);
            if result.is_err() {
                println!("error creating debug messenger: {}", result.unwrap_err())
            }
        }

        // TODO Extract surface creation into module

        // We need a handle to the surface loader so we can call the extension functions
        let (surface_loader, surface) = Self::create_win32_surface(&entry, &instance, &window);

        // TODO extract physical device selection into module
        let physical_device = match Self::pick_physical_device(&instance, &surface_loader, &surface)
        {
            Some(device) => device,
            None => panic!("No suitable physical device"),
        };

        // Extract device and queues into module
        let queue_families =
            Self::find_queue_families(&instance, &physical_device, &surface_loader, &surface);

        let logical_device = Self::create_logical_device(
            &instance,
            &physical_device,
            &queue_families,
            debug_config.is_some(),
        );

        let graphics_queue = Self::get_device_queue(
            &logical_device,
            queue_families
                .graphics_family
                .expect("Graphics queue family index"),
        );
        let present_queue = Self::get_device_queue(
            &logical_device,
            queue_families
                .present_family
                .expect("Present queue family index"),
        );

        let swapchain_data = Self::create_swap_chain(
            &instance,
            &logical_device,
            &surface_loader,
            &physical_device,
            &surface,
            &window,
            &queue_families,
        );

        let swapchain_image_views =
            Self::create_swapchain_image_views(&logical_device, &swapchain_data);

        let render_pass = Self::create_render_pass(
            &instance,
            physical_device,
            &logical_device,
            swapchain_data.format,
        );

        let descriptor_set_layout = Self::create_descriptor_set_layout(&logical_device);

        let (graphics_pipeline, pipeline_layout) = Self::create_graphics_pipeline(
            &logical_device,
            swapchain_data.extent,
            render_pass,
            descriptor_set_layout,
        );

        let command_pool = Self::create_command_pool(&logical_device, &queue_families);

        let physical_device_memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

        let (depth_image, depth_image_memory, depth_image_view) = Self::create_depth_resources(
            &instance,
            physical_device,
            &physical_device_memory_properties,
            &logical_device,
            graphics_queue,
            command_pool,
            swapchain_data.extent,
        );

        let swap_chain_frame_buffers = Self::create_frame_buffers(
            &logical_device,
            &swapchain_image_views,
            depth_image_view,
            swapchain_data.extent,
            render_pass,
        );

        let (vertex_buffer, vertex_buffer_memory) = Self::create_vertex_buffer(
            &instance,
            &logical_device,
            &QUAD_VERTICES,
            command_pool,
            graphics_queue,
            physical_device_memory_properties,
        );

        let (image, image_memory) = Self::create_texture_image(
            &logical_device,
            command_pool,
            graphics_queue,
            &physical_device_memory_properties,
            "src/textures/texture.jpg".into(),
        );

        let texture_image_view = Self::create_texture_image_view(&logical_device, image);

        let (index_buffer, index_buffer_memory) = Self::create_index_buffer(
            &instance,
            &logical_device,
            &QUAD_INDICES,
            command_pool,
            graphics_queue,
            physical_device_memory_properties,
        );

        let physical_device_properties =
            unsafe { instance.get_physical_device_properties(physical_device) };
        let texture_sampler =
            Self::create_texture_sampler(&logical_device, physical_device_properties);

        let (uniform_buffers, uniform_buffers_memory) = Self::create_uniform_buffers(
            &logical_device,
            physical_device_memory_properties,
            swapchain_image_views.len(),
        );

        let descriptor_pool =
            Self::create_descriptor_pool(&logical_device, swapchain_image_views.len());
        let descriptor_sets = Self::create_descriptor_sets(
            &logical_device,
            descriptor_pool,
            descriptor_set_layout,
            swapchain_image_views.len(),
        );
        Self::populate_descriptor_sets(
            &logical_device,
            &descriptor_sets,
            &uniform_buffers,
            texture_image_view,
            texture_sampler,
            swapchain_image_views.len(),
        );

        let command_buffers = Self::create_command_buffers(
            &logical_device,
            command_pool,
            render_pass,
            &swap_chain_frame_buffers,
            swapchain_data.extent,
            graphics_pipeline,
            vertex_buffer,
            index_buffer,
            pipeline_layout,
            &descriptor_sets,
        );

        // TODO: Handle image in flight fences
        let (image_available_semaphores, render_complete_semaphores, frame_fences) =
            Self::create_synchronisation_primitives(&logical_device);

        let image_fences: Vec<vk::Fence> = range(0, swapchain_data.images.len())
            .map(|_| vk::Fence::null())
            .collect();

        Self {
            _entry: entry,
            debug_config,
            instance,
            surface,
            surface_loader,
            physical_device,
            physical_device_memory_properties,
            queue_families,
            logical_device,
            graphics_queue,
            present_queue,
            swapchain_data,
            swapchain_image_views,
            render_pass,
            descriptor_pool,
            descriptor_sets,
            descriptor_set_layout,
            pipeline_layout,
            graphics_pipeline,
            swap_chain_frame_buffers,
            command_pool,
            command_buffers,
            image_available_semaphores,
            render_complete_semaphores,
            frame_fences,
            image_fences,
            current_frame: 0,
            window,
            frame_buffer_resized: false,
            vertex_buffer,
            vertex_buffer_memory,
            index_buffer,
            index_buffer_memory,
            uniform_buffers,
            uniform_buffers_memory,
            image,
            image_memory,
            texture_image_view,
            texture_sampler,
            start_time: Instant::now(),
            depth_image,
            depth_image_memory,
            depth_image_view,
        }
    }

    /**
    Instance creation
    */
    fn create_instance(
        entry: &ash::Entry,
        debug_config: &Option<debug::Configuration>,
    ) -> ash::Instance {
        let mut layers: Vec<CString> = Vec::new();
        let mut extensions = vec![Surface::name().to_owned(), Win32Surface::name().to_owned()];
        let mut extension_inputs = Vec::new();

        if let Some(configuration) = debug_config {
            let instance::Extension { name, data } = configuration.messenger_extension();
            extensions.push(name);
            extension_inputs.push(data);

            if let Ok(mut validation_layers) = configuration.instance_validation_layers(entry) {
                layers.append(&mut validation_layers)
            }
        }

        instance::new(entry, &layers, &extensions, &mut extension_inputs).unwrap()
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
                        Self::is_device_suitable(instance, device, surface_loader, surface)
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

        let required_device_extensions: Vec<String> = Self::get_device_extensions()
            .iter()
            .map(|&name| String::from(name.to_str().expect("Swapchain extension name")))
            .collect();
        let required_device_extensions_supported =
            Self::check_device_extension_support(&instance, device, required_device_extensions);

        println!(
            "Evaluating suitability of device [{}]",
            util::read_vk_string(&properties.device_name[..]).unwrap()
        );

        let supports_required_families =
            Self::find_queue_families(instance, device, surface_loader, surface).is_complete();

        if required_device_extensions_supported {
            // Only check swap chain support if the swap chain device extensions are supported
            let swap_chain_support =
                unsafe { Self::query_swap_chain_support(surface_loader, device, surface) };
            let swap_chain_adequate = !swap_chain_support.formats.is_empty()
                && !swap_chain_support.present_modes.is_empty();

            properties.device_type == vk::PhysicalDeviceType::DISCRETE_GPU
                && features.geometry_shader == 1
                && supports_required_families
                && swap_chain_adequate
                && features.sampler_anisotropy == 1
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
                    util::read_vk_string(&extension.extension_name[..])
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
        let device_features = vk::PhysicalDeviceFeatures::builder()
            .sampler_anisotropy(true)
            .build();

        let create_infos = &queue_create_infos[..];
        let required_validation_layer_raw_names: Vec<CString> = VALIDATION_LAYERS
            .iter()
            .map(|layer_name| CString::new(*layer_name).unwrap())
            .collect();
        let validation_layers: Vec<*const c_char> = required_validation_layer_raw_names
            .iter()
            .map(|layer_name| layer_name.as_ptr())
            .collect();
        let enabled_extension_names: Vec<*const c_char> = Self::get_device_extensions()
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
        let swap_chain_support =
            unsafe { Self::query_swap_chain_support(surface_loader, physical_device, surface) };
        let format = Self::choose_swap_surface_format(&swap_chain_support.formats);
        let present_mode = Self::choose_swap_present_mode(&swap_chain_support.present_modes);
        let extent = Self::choose_swap_extent(&swap_chain_support.capabilities, window);

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

    fn create_swapchain_image_views(
        device: &ash::Device,
        swapchain_data: &SwapChainData,
    ) -> Vec<vk::ImageView> {
        swapchain_data
            .images
            .iter()
            .map(|&image| {
                Self::create_image_view(
                    device,
                    image,
                    swapchain_data.format,
                    vk::ImageAspectFlags::COLOR,
                )
            })
            .collect()
    }

    fn create_render_pass(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        device: &ash::Device,
        swap_chain_format: vk::Format,
    ) -> vk::RenderPass {
        let color_attachment = vk::AttachmentDescription::builder()
            .format(swap_chain_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        let color_attachment_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let depth_attachment = vk::AttachmentDescription::builder()
            .format(Self::find_depth_format(instance, physical_device, device))
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .build();

        let depth_attachment_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
            .build();

        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&[color_attachment_ref])
            .depth_stencil_attachment(&depth_attachment_ref)
            .build();

        // Declare subpass dependencies
        let dependency = vk::SubpassDependency::builder()
            // Implicit subpass that always takes place
            .src_subpass(vk::SUBPASS_EXTERNAL)
            // Our subpass, index 0
            .dst_subpass(0)
            // Operation to wait on
            .src_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            // Stage that the operation occurs in
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            )
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_WRITE
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
            )
            .build();
        let subpass_dependencies = [dependency];

        let attachments = &[color_attachment, depth_attachment];
        let subpasses = &[subpass];
        let render_pass_ci = vk::RenderPassCreateInfo::builder()
            .attachments(attachments)
            .subpasses(subpasses)
            .dependencies(&subpass_dependencies);

        unsafe {
            device
                .create_render_pass(&render_pass_ci, None)
                .expect("render pass")
        }
    }

    fn create_descriptor_set_layout(device: &ash::Device) -> vk::DescriptorSetLayout {
        let ubo_layout_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
            .descriptor_count(1)
            .stage_flags(vk::ShaderStageFlags::VERTEX);
        let tex_sampler_layout_binding = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_count(1)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .stage_flags(vk::ShaderStageFlags::FRAGMENT);

        let bindings = [
            ubo_layout_binding.build(),
            tex_sampler_layout_binding.build(),
        ];
        let ci = vk::DescriptorSetLayoutCreateInfo::builder().bindings(&bindings);
        unsafe {
            device
                .create_descriptor_set_layout(&ci, None)
                .expect("Failed to create descriptor set layout!")
        }
    }

    fn create_graphics_pipeline(
        device: &ash::Device,
        swap_chain_extents: vk::Extent2D,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let vert_path = Path::new(env!("OUT_DIR")).join("vert.spv");
        println!(
            "Reading vertex shader from {}",
            vert_path.to_str().expect("vertex shader path")
        );
        let vert_shader_code = util::read_shader_code(vert_path.as_path());
        let frag_path = Path::new(env!("OUT_DIR")).join("frag.spv");
        println!(
            "Reading frag shader from {}",
            frag_path.to_str().expect("frag shader path")
        );
        let frag_shader_code = util::read_shader_code(frag_path.as_path());

        let vert_shader_module = Self::create_shader_module(device, &vert_shader_code);
        let frag_shader_module = Self::create_shader_module(device, &frag_shader_code);

        let main_fn_name = CString::new("main").unwrap();
        let vert_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vert_shader_module)
            .name(main_fn_name.as_c_str());
        let frag_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(frag_shader_module)
            .name(main_fn_name.as_c_str());
        let shader_stages = vec![vert_stage_builder.build(), frag_stage_builder.build()];

        let binding_description = [Vertex::get_binding_desription()];
        let attribute_descriptions = Vertex::get_attribute_descriptions();
        // Describe our vertex layout, the input for the vertex shader
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&binding_description)
            .vertex_attribute_descriptions(&attribute_descriptions);

        // Describe the primitives we are drawing with our vertices
        let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::builder()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
            .primitive_restart_enable(false);

        // Describe the region of the framebuffer that we want to render to
        let viewport = vk::Viewport::builder()
            .x(0.0)
            .y(0.0)
            .min_depth(0.0)
            .max_depth(1.0)
            .width(swap_chain_extents.width as f32)
            .height(swap_chain_extents.height as f32);

        // Clipping filter for frame buffer. We don't want to clip the frame buffer with this pipeline so we do the entire frame buffer.
        let scissor = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(swap_chain_extents);

        let viewports = [viewport.build()];
        let scissors = [scissor.build()];
        let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
            .viewports(&viewports)
            .scissors(&scissors);

        // Set up a rasterizer
        let rasterizer = vk::PipelineRasterizationStateCreateInfo::builder()
            .depth_clamp_enable(false) // Clip beyond near and far planes
            .rasterizer_discard_enable(false) // Don't skip rasterization
            .polygon_mode(vk::PolygonMode::FILL) // Rasterize entire polygon
            .line_width(1.0) // Rasterization line width
            .cull_mode(vk::CullModeFlags::BACK) // Face culling
            .front_face(vk::FrontFace::CLOCKWISE) // Vertex direction to determine if face is front or back
            .depth_bias_enable(false); // Don't alter depth values with bias

        // MSAA config. Ignored for now.
        let multisampling = vk::PipelineMultisampleStateCreateInfo::builder()
            .sample_shading_enable(false)
            .rasterization_samples(vk::SampleCountFlags::TYPE_1)
            .min_sample_shading(1.0)
            .alpha_to_coverage_enable(false)
            .alpha_to_one_enable(false);

        // TODO Set up alpha blending
        let color_blend_attachment = vk::PipelineColorBlendAttachmentState::builder()
            .color_write_mask(vk::ColorComponentFlags::all())
            .blend_enable(false)
            .build();
        let color_blend_attachments = [color_blend_attachment];
        let global_blend = vk::PipelineColorBlendStateCreateInfo::builder()
            .logic_op_enable(false)
            .attachments(&color_blend_attachments);

        let depth_stencil_attachment = vk::PipelineDepthStencilStateCreateInfo::builder()
            .depth_test_enable(true)
            .depth_write_enable(true)
            .depth_compare_op(vk::CompareOp::LESS)
            .depth_bounds_test_enable(false)
            .min_depth_bounds(0.0)
            .max_depth_bounds(0.0)
            .stencil_test_enable(false);

        let dynamic_states = &[vk::DynamicState::VIEWPORT, vk::DynamicState::LINE_WIDTH];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(dynamic_states);

        let set_layouts = [descriptor_set_layout];
        let pipeline_layout_info =
            vk::PipelineLayoutCreateInfo::builder().set_layouts(&set_layouts);
        let pipeline_layout = unsafe {
            device
                .create_pipeline_layout(&pipeline_layout_info, None)
                .expect("pipeline layout")
        };

        let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
            .stages(&shader_stages[..])
            .vertex_input_state(&vertex_input_info)
            .input_assembly_state(&input_assembly_info)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterizer)
            .multisample_state(&multisampling)
            .color_blend_state(&global_blend)
            .depth_stencil_state(&depth_stencil_attachment)
            .layout(pipeline_layout)
            .render_pass(render_pass);

        let pipelines = unsafe {
            device
                .create_graphics_pipelines(
                    vk::PipelineCache::null(),
                    &[pipeline_info.build()],
                    None,
                )
                .expect("graphics pipeline")
        };

        unsafe { device.destroy_shader_module(vert_shader_module, None) };
        unsafe { device.destroy_shader_module(frag_shader_module, None) };

        (pipelines[0], pipeline_layout)
    }

    fn create_shader_module(device: &ash::Device, code: &[u32]) -> vk::ShaderModule {
        let builder = vk::ShaderModuleCreateInfo::builder().code(code);
        unsafe {
            device
                .create_shader_module(&builder, None)
                .expect("Shader module")
        }
    }

    fn create_frame_buffers(
        device: &ash::Device,
        swapchain_image_views: &Vec<vk::ImageView>,
        depth_image_view: vk::ImageView,
        swapchain_extent: vk::Extent2D,
        render_pass: vk::RenderPass,
    ) -> Vec<vk::Framebuffer> {
        // Create a frame bufffer for each swap chain image
        swapchain_image_views
            .iter()
            .map(|&image_view| {
                let attachments = [image_view, depth_image_view];

                let builder = vk::FramebufferCreateInfo::builder()
                    // Which render pass this buffer is for
                    .render_pass(render_pass)
                    // The images to pass to the render pass - will be bound to render pass image attachments
                    .attachments(&attachments)
                    .width(swapchain_extent.width)
                    .height(swapchain_extent.height)
                    .layers(1);

                unsafe {
                    device
                        .create_framebuffer(&builder, None)
                        .expect("Frame buffer for image view")
                }
            })
            .collect()
    }

    /// Creates a command pool - a vulkan structure to manage the memory for storing buggers and command buffers
    /// allocated by them.
    fn create_command_pool(
        device: &ash::Device,
        queue_indices: &QueueFamilyIndices,
    ) -> vk::CommandPool {
        let ci = vk::CommandPoolCreateInfo::builder()
            // Which queue will this command pool create command buffers for
            .queue_family_index(
                queue_indices
                    .graphics_family
                    .expect("Graphics queue family"),
            );

        unsafe {
            device
                .create_command_pool(&ci, None)
                .expect("Graphics command pool")
        }
    }

    fn create_vertex_buffer(
        instance: &ash::Instance,
        device: &ash::Device,
        vertex_data: &[Vertex],
        command_pool: vk::CommandPool,
        submit_queue: vk::Queue,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let size: u64 = (mem::size_of::<Vertex>() * vertex_data.len())
            .try_into()
            .unwrap();

        let (staging_buffer, staging_buffer_memory) = Self::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            &device_memory_properties,
        );

        unsafe {
            let data_ptr = device
                .map_memory(staging_buffer_memory, 0, size, vk::MemoryMapFlags::empty())
                .expect("Failed to Map staging buffer Memory")
                as *mut Vertex;

            data_ptr.copy_from_nonoverlapping(QUAD_VERTICES.as_ptr(), QUAD_VERTICES.len());

            device.unmap_memory(staging_buffer_memory);
        }

        let (vertex_buffer, vertex_buffer_memory) = Self::create_buffer(
            device,
            size,
            vk::BufferUsageFlags::VERTEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device_memory_properties,
        );

        Self::copy_buffer(
            device,
            submit_queue,
            command_pool,
            staging_buffer,
            vertex_buffer,
            size,
        );

        unsafe { device.destroy_buffer(staging_buffer, None) };
        unsafe { device.free_memory(staging_buffer_memory, None) };

        (vertex_buffer, vertex_buffer_memory)
    }

    // TODO: Create generic "create device local buffer" method. Usage should be parameter.
    fn create_index_buffer(
        instance: &ash::Instance,
        device: &ash::Device,
        index_data: &[u16],
        command_pool: vk::CommandPool,
        submit_queue: vk::Queue,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let length = index_data.len();
        if length == 0 {
            panic!("Empy index data")
        }
        let size = mem::size_of::<u16>() * index_data.len();

        let (staging_buffer, staging_buffer_memory) = Self::create_buffer(
            device,
            size as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_COHERENT | vk::MemoryPropertyFlags::HOST_VISIBLE,
            &device_memory_properties,
        );

        unsafe {
            let data_ptr = device
                .map_memory(
                    staging_buffer_memory,
                    0,
                    size as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .expect("Failed to Map staging buffer Memory")
                as *mut u16;

            data_ptr.copy_from_nonoverlapping(QUAD_INDICES.as_ptr(), QUAD_INDICES.len());

            device.unmap_memory(staging_buffer_memory);
        }

        let (index_buffer, index_buffer_memory) = Self::create_buffer(
            device,
            size as u64,
            vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            &device_memory_properties,
        );

        Self::copy_buffer(
            device,
            submit_queue,
            command_pool,
            staging_buffer,
            index_buffer,
            size as u64,
        );

        unsafe { device.destroy_buffer(staging_buffer, None) };
        unsafe { device.free_memory(staging_buffer_memory, None) };

        (index_buffer, index_buffer_memory)
    }

    fn create_uniform_buffers(
        device: &ash::Device,
        device_memory_properties: vk::PhysicalDeviceMemoryProperties,
        num_buffers: usize,
    ) -> (Vec<vk::Buffer>, Vec<vk::DeviceMemory>) {
        let buffer_size = mem::size_of::<UniformBufferObject>() as u64;

        let memory_properties =
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;

        num::range(0, num_buffers)
            .map(|_| {
                Self::create_buffer(
                    device,
                    buffer_size,
                    vk::BufferUsageFlags::UNIFORM_BUFFER,
                    memory_properties,
                    &device_memory_properties,
                )
            })
            .unzip()
    }

    fn create_buffer(
        device: &ash::Device,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        required_memory_properties: vk::MemoryPropertyFlags,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let ci = vk::BufferCreateInfo::builder()
            .size(size as u64)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&ci, None)
                .expect("Creating vertex buffer")
        };

        let mem_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let suitable_memory_type = Self::find_memory_type(
            mem_requirements.memory_type_bits,
            required_memory_properties,
            device_memory_properties,
        );

        let alloc_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(mem_requirements.size)
            .memory_type_index(suitable_memory_type);

        let buffer_memory = unsafe {
            device
                .allocate_memory(&alloc_info, None)
                .expect("Allocatin vertex buffer memory")
        };
        unsafe {
            device
                .bind_buffer_memory(buffer, buffer_memory, 0)
                .expect("Bind buffer memory");
        };

        (buffer, buffer_memory)
    }

    fn find_memory_type(
        type_filter: u32,
        required_properties: vk::MemoryPropertyFlags,
        mem_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> u32 {
        for (i, memory_type) in mem_properties.memory_types.iter().enumerate() {
            // type_filter are the physical device memory types that we want for our buffer
            if (type_filter & (1 << i)) > 0
                && memory_type.property_flags.contains(required_properties)
            {
                return i as u32;
            }
        }

        panic!("Failed to find suitable memory type!")
    }

    fn copy_buffer(
        device: &ash::Device,
        queue: vk::Queue,
        pool: vk::CommandPool,
        source: vk::Buffer,
        destination: vk::Buffer,
        size: vk::DeviceSize,
    ) {
        let command_buffer = begin_single_time_commands(device, pool);

        let copy_regions = [vk::BufferCopy::builder()
            .src_offset(0)
            .dst_offset(0)
            .size(size)
            .build()];

        unsafe {
            device.cmd_copy_buffer(command_buffer, source, destination, &copy_regions);
        };

        end_single_time_commands(device, pool, command_buffer, queue);
    }

    fn create_descriptor_pool(device: &ash::Device, size: usize) -> vk::DescriptorPool {
        let pool_sizes = [
            vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::UNIFORM_BUFFER)
                .descriptor_count(size as u32)
                .build(),
            vk::DescriptorPoolSize::builder()
                .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(size as u32)
                .build(),
        ];

        // We can set a flag that allows us to free descriptor sets, but we won't need that
        let ci = vk::DescriptorPoolCreateInfo::builder()
            .pool_sizes(&pool_sizes)
            .max_sets(size as u32);

        unsafe {
            device
                .create_descriptor_pool(&ci, None)
                .expect("Creating descriptor pool")
        }
    }

    fn create_descriptor_sets(
        device: &ash::Device,
        pool: vk::DescriptorPool,
        layout_template: vk::DescriptorSetLayout,
        size: usize,
    ) -> Vec<vk::DescriptorSet> {
        let mut layouts: Vec<vk::DescriptorSetLayout> = Vec::new();

        // Every frame uses the same descriptor layout
        for _ in 0..size {
            layouts.push(layout_template);
        }
        let alloc_info = vk::DescriptorSetAllocateInfo::builder()
            .descriptor_pool(pool)
            .set_layouts(&layouts);

        unsafe {
            device
                .allocate_descriptor_sets(&alloc_info)
                .expect("allocating descriptor sets")
        }
    }

    fn populate_descriptor_sets(
        device: &ash::Device,
        descriptor_sets: &Vec<vk::DescriptorSet>,
        uniform_buffers: &Vec<vk::Buffer>,
        texture_image_view: vk::ImageView,
        texture_sampler: vk::Sampler,
        size: usize,
    ) {
        for i in 0..size {
            let bi = [vk::DescriptorBufferInfo::builder()
                .buffer(uniform_buffers[i])
                .offset(0)
                .range(mem::size_of::<UniformBufferObject>() as u64)
                .build()];

            let image_info = [vk::DescriptorImageInfo::builder()
                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .image_view(texture_image_view)
                .sampler(texture_sampler)
                .build()];

            let write = [
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(0)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::UNIFORM_BUFFER)
                    .buffer_info(&bi)
                    .build(),
                vk::WriteDescriptorSet::builder()
                    .dst_set(descriptor_sets[i])
                    .dst_binding(1)
                    .dst_array_element(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .image_info(&image_info)
                    .build(),
            ];

            unsafe { device.update_descriptor_sets(&write, &[]) };
        }
    }

    /// Allocates `num_buffers` command buffers to the given command pool on the given device. Records all commands required to render a frame from
    /// the vertex and index data.
    fn create_command_buffers(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        frame_buffers: &Vec<vk::Framebuffer>,
        swap_chain_extent: vk::Extent2D,
        graphics_pipeline: vk::Pipeline,
        vertex_buffer: vk::Buffer,
        index_buffer: vk::Buffer,
        pipeline_layout: vk::PipelineLayout,
        descriptor_sets: &Vec<vk::DescriptorSet>,
    ) -> Vec<vk::CommandBuffer> {
        let num_buffers = frame_buffers.len();
        if frame_buffers.len() != num_buffers {
            panic!("Must have same number of command buffers as frame buffers")
        }

        let ci = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            // Primary command buffer is submitted directly to queue, cannot be called from other command buffers.
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(num_buffers as u32);

        let buffers = unsafe {
            device
                .allocate_command_buffers(&ci)
                .expect("Command buffers")
        };

        for i in range(0, num_buffers) {
            let index = i as usize;
            let buffer = buffers[index];
            let frame_buffer = frame_buffers[index];

            let bi = vk::CommandBufferBeginInfo::builder();

            unsafe {
                device
                    .begin_command_buffer(buffer, &bi)
                    .expect("Recording command buffer")
            };

            let clear_values = [
                vk::ClearValue {
                    color: vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                },
                vk::ClearValue {
                    depth_stencil: vk::ClearDepthStencilValue {
                        depth: 1.0,
                        stencil: 0,
                    },
                },
            ];

            let render_pass_bi = vk::RenderPassBeginInfo::builder()
                .render_pass(render_pass)
                .framebuffer(frame_buffer)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: swap_chain_extent,
                })
                .clear_values(&clear_values);

            unsafe {
                // Inline means render pass commands will be in primary command buffer as opposed to SECONDARY_COMMAND_BUFFERS
                // where render pass commands are in secondary buffer
                device.cmd_begin_render_pass(buffer, &render_pass_bi, vk::SubpassContents::INLINE);
                device.cmd_bind_pipeline(
                    buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    graphics_pipeline,
                );

                let buffers = [vertex_buffer];
                let offsets = [0];
                device.cmd_bind_vertex_buffers(buffer, 0, &buffers, &offsets);
                device.cmd_bind_index_buffer(buffer, index_buffer, 0, vk::IndexType::UINT16);

                let sets = [descriptor_sets[i]];
                device.cmd_bind_descriptor_sets(
                    buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline_layout,
                    0,
                    &sets,
                    &[],
                );

                device.cmd_draw_indexed(buffer, QUAD_INDICES.len() as u32, 1, 0, 0, 0);

                device.cmd_end_render_pass(buffer);

                device
                    .end_command_buffer(buffer)
                    .expect("Ending command buffer")
            }
        }

        buffers
    }

    fn create_synchronisation_primitives(
        device: &ash::Device,
    ) -> (Vec<vk::Semaphore>, Vec<vk::Semaphore>, Vec<vk::Fence>) {
        let mut image_available_semaphores: Vec<vk::Semaphore> = Vec::new();
        let mut render_complete_semaphores: Vec<vk::Semaphore> = Vec::new();
        let mut in_flight_fences: Vec<vk::Fence> = Vec::new();

        for _ in num::range(0, MAX_FRAMES_IN_FLIGHT) {
            let (image_semaphore, render_semaphore, frame_fence) = unsafe {
                (
                    device
                        .create_semaphore(&vk::SemaphoreCreateInfo::builder(), None)
                        .expect("Image Semaphore"),
                    device
                        .create_semaphore(&vk::SemaphoreCreateInfo::builder(), None)
                        .expect("Render Semaphore"),
                    device
                        .create_fence(
                            &vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED),
                            None,
                        )
                        .expect("Frame fence"),
                )
            };
            image_available_semaphores.push(image_semaphore);
            render_complete_semaphores.push(render_semaphore);
            in_flight_fences.push(frame_fence);
        }

        (
            image_available_semaphores,
            render_complete_semaphores,
            in_flight_fences,
        )
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

    /**
     * recreate_swapchain re-creates the swapchain and all structures that are dependent on it.
     */
    fn recreate_swapchain(&mut self) {
        unsafe {
            self.logical_device
                .device_wait_idle()
                .expect("Waiting for device to be idle")
        };

        self.cleanup_swapchain();

        let swapchain_data = Self::create_swap_chain(
            &self.instance,
            &self.logical_device,
            &self.surface_loader,
            &self.physical_device,
            &self.surface,
            &self.window,
            &self.queue_families,
        );
        self.swapchain_data = swapchain_data;

        self.swapchain_image_views =
            Self::create_swapchain_image_views(&self.logical_device, &self.swapchain_data);

        self.render_pass = Self::create_render_pass(
            &self.instance,
            self.physical_device,
            &self.logical_device,
            self.swapchain_data.format,
        );

        let (graphics_pipeline, pipeline_layout) = Self::create_graphics_pipeline(
            &self.logical_device,
            self.swapchain_data.extent,
            self.render_pass,
            self.descriptor_set_layout,
        );
        self.graphics_pipeline = graphics_pipeline;
        self.pipeline_layout = pipeline_layout;

        (
            self.depth_image,
            self.depth_image_memory,
            self.depth_image_view,
        ) = Self::create_depth_resources(
            &self.instance,
            self.physical_device,
            &self.physical_device_memory_properties,
            &self.logical_device,
            self.graphics_queue,
            self.command_pool,
            self.swapchain_data.extent,
        );

        self.swap_chain_frame_buffers = Self::create_frame_buffers(
            &self.logical_device,
            &self.swapchain_image_views,
            self.depth_image_view,
            self.swapchain_data.extent,
            self.render_pass,
        );

        let (uniform_buffers, uniform_buffers_memory) = Self::create_uniform_buffers(
            &self.logical_device,
            self.physical_device_memory_properties,
            self.swapchain_image_views.len(),
        );
        self.uniform_buffers = uniform_buffers;
        self.uniform_buffers_memory = uniform_buffers_memory;

        self.descriptor_pool =
            Self::create_descriptor_pool(&self.logical_device, self.swapchain_image_views.len());
        self.descriptor_sets = Self::create_descriptor_sets(
            &self.logical_device,
            self.descriptor_pool,
            self.descriptor_set_layout,
            self.swapchain_image_views.len(),
        );
        Self::populate_descriptor_sets(
            &self.logical_device,
            &self.descriptor_sets,
            &self.uniform_buffers,
            self.texture_image_view,
            self.texture_sampler,
            self.swapchain_image_views.len(),
        );

        self.command_buffers = Self::create_command_buffers(
            &self.logical_device,
            self.command_pool,
            self.render_pass,
            &self.swap_chain_frame_buffers,
            self.swapchain_data.extent,
            self.graphics_pipeline,
            self.vertex_buffer,
            self.index_buffer,
            self.pipeline_layout,
            &self.descriptor_sets,
        );
    }

    fn cleanup_swapchain(&mut self) {
        unsafe {
            for &frame_buffer in self.swap_chain_frame_buffers.iter() {
                self.logical_device.destroy_framebuffer(frame_buffer, None)
            }

            for &buffer in self.uniform_buffers.iter() {
                self.logical_device.destroy_buffer(buffer, None)
            }

            for &buffer_memory in self.uniform_buffers_memory.iter() {
                self.logical_device.free_memory(buffer_memory, None)
            }

            self.logical_device
                .destroy_image_view(self.depth_image_view, None);
            self.logical_device.destroy_image(self.depth_image, None);
            self.logical_device
                .free_memory(self.depth_image_memory, None);

            self.logical_device
                .destroy_descriptor_pool(self.descriptor_pool, None);

            self.logical_device
                .free_command_buffers(self.command_pool, &self.command_buffers);

            self.logical_device
                .destroy_pipeline(self.graphics_pipeline, None);
            self.logical_device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.logical_device
                .destroy_render_pass(self.render_pass, None);

            for &image_view in self.swapchain_image_views.iter() {
                self.logical_device.destroy_image_view(image_view, None)
            }
            self.swapchain_data
                .loader
                .destroy_swapchain(self.swapchain_data.swapchain, None);
        }
    }

    // TODO: Semaphores not in consistent state when re-creating swapchain when frame buffer is suboptimal
    fn draw_frame(&mut self) {
        // TODO: Wait for fences
        let current_frame_fences = [self.frame_fences[self.current_frame]];
        unsafe {
            self.logical_device
                .wait_for_fences(&current_frame_fences, true, u64::MAX)
                .expect("Waiting for frame fence");
        };

        // Request an image from the swap chain. It will signal the given semaphore when the image is ready
        let (image_index, recreated) = unsafe {
            match self.swapchain_data.loader.acquire_next_image(
                self.swapchain_data.swapchain,
                u64::MAX,
                self.image_available_semaphores[self.current_frame],
                vk::Fence::null(),
            ) {
                Ok((idx, _)) => (idx as usize, false),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.recreate_swapchain();
                    (0 as usize, true)
                }
                Err(_) => panic!("Failed to acquire swapchain image"),
            }
        };

        // If the swapchain had to be re-created, exit early and draw again in the next tick.
        if recreated {
            return;
        }

        self.update_uniform_buffer(image_index);

        // Make sure we don't reference a swapchain image that is already being presented
        if self.image_fences[image_index] != vk::Fence::null() {
            let active_image_in_flight_fences = [self.image_fences[image_index]];
            unsafe {
                self.logical_device
                    .wait_for_fences(&active_image_in_flight_fences, true, u64::MAX)
                    .expect("Image in flight fence");
            };
        };
        self.image_fences[image_index] = self.frame_fences[self.current_frame];

        let render_wait_semaphores = [self.image_available_semaphores[self.current_frame]];
        let render_signal_semaphores = [self.render_complete_semaphores[self.current_frame]];
        let wait_stage = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let command_buffers = [self.command_buffers[image_index]];

        // Submit info is data representing a request to a queue and how to synchronise it with other requests
        // Tells vulkan to wait at the "color attachment" point until the image_available_semaphore has signaled,
        // then run the command buffer. Once the commands are complete, signal the "render_complete_semaphore".
        let submit_info = vk::SubmitInfo::builder()
            .wait_semaphores(&render_wait_semaphores)
            .wait_dst_stage_mask(&wait_stage)
            .command_buffers(&command_buffers)
            .signal_semaphores(&render_signal_semaphores);

        let queue_submissions = [submit_info.build()];

        unsafe {
            self.logical_device
                .reset_fences(&current_frame_fences)
                .expect("Resetting current frame fence");
            self.logical_device
                .queue_submit(
                    self.graphics_queue,
                    &queue_submissions,
                    self.frame_fences[self.current_frame],
                )
                .expect("Graphics queue submit")
        };

        let present_wait_semaphores = render_signal_semaphores;
        let swapchains = [self.swapchain_data.swapchain];
        let image_indices = [image_index as u32];
        let present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&present_wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        let present_result = unsafe {
            self.swapchain_data
                .loader
                .queue_present(self.present_queue, &present_info.build())
        };

        match unsafe { self.logical_device.queue_wait_idle(self.present_queue) } {
            Ok(_) => {}
            Err(result) => {
                println!("Error waiting for present queue: {}", result)
            }
        };

        match (present_result, self.frame_buffer_resized) {
            (_, true) => {
                // self.recreate_swapchain();
                self.frame_buffer_resized = false;
            }
            (Ok(_), _) => (),
            // (Ok(false), _) | (Err(vk::Result::ERROR_OUT_OF_DATE_KHR), _) => {
            //     self.recreate_swapchain();
            //     return;
            // }
            (Err(_), _) => panic!("Failed to present swapchain image"),
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
    }

    fn update_uniform_buffer(&self, current_image: usize) {
        let current_time = Instant::now();
        let time = current_time - self.start_time;

        let rot = Matrix4::from(Euler {
            x: Deg(0f32),
            y: Deg(0f32),
            z: Deg(45f32) * time.as_secs_f32(),
        });
        let view = Matrix4::<f32>::look_at_rh(
            Point3::new(2.0, 2.0, 2.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 0.0, 1.0),
        );
        let extent = self.swapchain_data.extent;
        let aspect_ratio = extent.width as f32 / extent.height as f32;
        let proj = cgmath::perspective(Deg(45.0), aspect_ratio, 0.1, 10.0);

        // We put them in an array so we can get a raw pointer to this data.
        let ubos = [UniformBufferObject {
            model: rot,
            view,
            perspective: proj,
        }];

        let buffer_size = (std::mem::size_of::<UniformBufferObject>() * ubos.len()) as u64;

        unsafe {
            let data_ptr =
                self.logical_device
                    .map_memory(
                        self.uniform_buffers_memory[current_image],
                        0,
                        buffer_size,
                        vk::MemoryMapFlags::empty(),
                    )
                    .expect("Failed to Map Memory") as *mut UniformBufferObject;

            data_ptr.copy_from_nonoverlapping(ubos.as_ptr(), ubos.len());

            self.logical_device
                .unmap_memory(self.uniform_buffers_memory[current_image]);
        }
    }

    fn main_loop(mut self, event_loop: EventLoop<()>) {
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
                Event::WindowEvent {
                    event: WindowEvent::Resized(_),
                    ..
                } => self.frame_buffer_resized = true,
                Event::MainEventsCleared => {
                    // Application update code.
                    // Queue a RedrawRequested event.
                    //
                    // You only need to call this if you've determined that you need to redraw, in
                    // applications which do not always need to. Applications that redraw continuously
                    // can just render here instead.

                    self.window.request_redraw()
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

    fn run(self, event_loop: EventLoop<()>) {
        self.main_loop(event_loop);
    }

    fn create_texture_image(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        image_path: String,
    ) -> (vk::Image, vk::DeviceMemory) {
        let mut image_object = image::open(image_path).unwrap(); // this function is slow in debug mode.

        // Why flipv?
        image_object = image_object.flipv();

        let (image_width, image_height) = (image_object.width(), image_object.height());
        let image_size =
            (std::mem::size_of::<u8>() as u32 * image_width * image_height * 4) as vk::DeviceSize;
        let image_data = match &image_object {
            image::DynamicImage::ImageLuma8(_) | image::DynamicImage::ImageRgb8(_) => {
                image_object.to_rgba8().into_raw()
            }
            image::DynamicImage::ImageLumaA8(_) | image::DynamicImage::ImageRgba8(_) => {
                image_object.to_rgba8().into_raw()
            }
            image_type => panic!("Unsupported image type: {:?}", image_type),
        };

        if image_size <= 0 {
            panic!("Failed to load texture image!")
        }

        let (staging_buffer, staging_mem) = Self::create_buffer(
            device,
            image_size,
            vk::BufferUsageFlags::TRANSFER_SRC,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            device_memory_properties,
        );

        unsafe {
            let data = device
                .map_memory(staging_mem, 0, image_size, MemoryMapFlags::empty())
                .expect("Map memory for image staging buffer") as *mut u8;

            data.copy_from_nonoverlapping(image_data.as_ptr(), image_data.len());
            device.unmap_memory(staging_mem);
        }

        let (image, image_memory) = Self::create_image(
            device,
            image_width,
            image_height,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            device_memory_properties,
        );

        Self::transition_image_layout(
            device,
            queue,
            command_pool,
            image,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        Self::copy_buffer_to_image(
            device,
            command_pool,
            queue,
            staging_buffer,
            image,
            image_width,
            image_height,
        );

        Self::transition_image_layout(
            device,
            queue,
            command_pool,
            image,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        );

        unsafe {
            device.destroy_buffer(staging_buffer, None);
            device.free_memory(staging_mem, None);
        }

        (image, image_memory)
    }

    fn create_image(
        device: &ash::Device,
        width: u32,
        height: u32,
        format: vk::Format,
        tiling: vk::ImageTiling,
        usage: vk::ImageUsageFlags,
        memory_properties: vk::MemoryPropertyFlags,
        device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
    ) -> (vk::Image, vk::DeviceMemory) {
        let image_ci = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .extent(
                vk::Extent3D::builder()
                    .width(width)
                    .height(height)
                    .depth(1)
                    .build(),
            )
            .mip_levels(1)
            .array_layers(1)
            .format(format)
            .tiling(tiling)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .samples(vk::SampleCountFlags::TYPE_1)
            .flags(vk::ImageCreateFlags::empty());

        let image = unsafe {
            device
                .create_image(&image_ci, None)
                .expect("Creating texture image")
        };

        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };

        let image_ai = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(Self::find_memory_type(
                memory_requirements.memory_type_bits,
                memory_properties,
                device_memory_properties,
            ));
        let image_mem = unsafe {
            let mem = device
                .allocate_memory(&image_ai, None)
                .expect("Allocating image memory");
            device
                .bind_image_memory(image, mem, 0)
                .expect("Binding image memory");
            mem
        };

        (image, image_mem)
    }

    fn transition_image_layout(
        device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        image: vk::Image,
        format: vk::Format,
        old: vk::ImageLayout,
        new: vk::ImageLayout,
    ) {
        let command_buffer = begin_single_time_commands(device, command_pool);

        let (src_access_mask, dst_access_mask, src_stage, dst_stage): (
            vk::AccessFlags,
            vk::AccessFlags,
            vk::PipelineStageFlags,
            vk::PipelineStageFlags,
        ) = match (old, new) {
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::TRANSFER_DST_OPTIMAL) => (
                vk::AccessFlags::empty(),
                vk::AccessFlags::TRANSFER_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
            ),
            (vk::ImageLayout::TRANSFER_DST_OPTIMAL, vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL) => (
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::SHADER_READ,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
            ),
            (vk::ImageLayout::UNDEFINED, vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL) => (
                vk::AccessFlags::empty(),
                vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                    | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::EARLY_FRAGMENT_TESTS,
            ),
            _ => panic!("Unsupported layout transition"),
        };

        let mut aspect_mask = vk::ImageAspectFlags::COLOR;
        if new.eq(&vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL) {
            aspect_mask = vk::ImageAspectFlags::DEPTH;

            if Self::has_stencil_component(format) {
                aspect_mask |= vk::ImageAspectFlags::STENCIL;
            }
        }

        let barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(old)
            .new_layout(new)
            .src_access_mask(src_access_mask)
            .dst_access_mask(dst_access_mask)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(aspect_mask)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            );

        unsafe {
            device.cmd_pipeline_barrier(
                command_buffer,
                src_stage,
                dst_stage,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier.build()],
            )
        }

        end_single_time_commands(device, command_pool, command_buffer, queue);
    }

    fn create_image_view(
        device: &ash::Device,
        image: vk::Image,
        format: vk::Format,
        aspect_flags: vk::ImageAspectFlags,
    ) -> vk::ImageView {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(aspect_flags)
                    .base_mip_level(0)
                    .level_count(1)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .components(vk::ComponentMapping {
                r: vk::ComponentSwizzle::IDENTITY,
                g: vk::ComponentSwizzle::IDENTITY,
                b: vk::ComponentSwizzle::IDENTITY,
                a: vk::ComponentSwizzle::IDENTITY,
            });

        unsafe {
            device
                .create_image_view(&create_info, None)
                .expect("Creating texture image view")
        }
    }

    fn copy_buffer_to_image(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        buffer: vk::Buffer,
        image: vk::Image,
        width: u32,
        height: u32,
    ) {
        let command_buffer = begin_single_time_commands(device, command_pool);

        let region = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(0)
                    .base_array_layer(0)
                    .layer_count(1)
                    .build(),
            )
            .image_offset(vk::Offset3D::builder().x(0).y(0).z(0).build())
            .image_extent(
                vk::Extent3D::builder()
                    .width(width)
                    .height(height)
                    .depth(1)
                    .build(),
            );

        unsafe {
            device.cmd_copy_buffer_to_image(
                command_buffer,
                buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region.build()],
            );
        }

        end_single_time_commands(device, command_pool, command_buffer, queue);
    }

    fn create_texture_image_view(device: &ash::Device, image: vk::Image) -> vk::ImageView {
        Self::create_image_view(
            device,
            image,
            vk::Format::R8G8B8A8_SRGB,
            vk::ImageAspectFlags::COLOR,
        )
    }

    fn create_texture_sampler(
        device: &ash::Device,
        physical_device_properties: vk::PhysicalDeviceProperties,
    ) -> vk::Sampler {
        let create_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .address_mode_u(vk::SamplerAddressMode::REPEAT)
            .address_mode_v(vk::SamplerAddressMode::REPEAT)
            .address_mode_w(vk::SamplerAddressMode::REPEAT)
            .anisotropy_enable(true)
            .max_anisotropy(physical_device_properties.limits.max_sampler_anisotropy)
            .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
            .unnormalized_coordinates(false)
            .compare_enable(false)
            .compare_op(vk::CompareOp::ALWAYS)
            .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
            .mip_lod_bias(0f32)
            .min_lod(0f32)
            .max_lod(0f32);

        unsafe {
            device
                .create_sampler(&create_info, None)
                .expect("Creating texture sampler")
        }
    }

    fn create_depth_resources(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        physical_device_memory_properties: &vk::PhysicalDeviceMemoryProperties,
        logical_device: &ash::Device,
        queue: vk::Queue,
        command_pool: vk::CommandPool,
        extent: vk::Extent2D,
    ) -> (vk::Image, vk::DeviceMemory, vk::ImageView) {
        let format = Self::find_depth_format(instance, physical_device, logical_device);

        let (image, image_memory) = Self::create_image(
            logical_device,
            extent.width,
            extent.height,
            format,
            vk::ImageTiling::OPTIMAL,
            vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            physical_device_memory_properties,
        );

        let image_view =
            Self::create_image_view(logical_device, image, format, vk::ImageAspectFlags::DEPTH);

        Self::transition_image_layout(
            logical_device,
            queue,
            command_pool,
            image,
            format,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        );

        (image, image_memory, image_view)
    }

    fn find_depth_format(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        logical_device: &ash::Device,
    ) -> vk::Format {
        Self::find_supported_format(
            instance,
            physical_device,
            logical_device,
            vec![
                vk::Format::D32_SFLOAT,
                vk::Format::D32_SFLOAT_S8_UINT,
                vk::Format::D24_UNORM_S8_UINT,
            ]
            .iter(),
            vk::ImageTiling::OPTIMAL,
            vk::FormatFeatureFlags::DEPTH_STENCIL_ATTACHMENT,
        )
        .expect("getting depth format")
    }

    fn find_supported_format(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        logical_device: &ash::Device,
        candidates: impl Iterator<Item = impl Deref<Target = vk::Format>>,
        tiling: vk::ImageTiling,
        features: vk::FormatFeatureFlags,
    ) -> Option<vk::Format> {
        for format in candidates {
            let props = unsafe {
                instance.get_physical_device_format_properties(physical_device, format.clone())
            };
            if tiling.eq(&vk::ImageTiling::LINEAR)
                && props.linear_tiling_features.contains(features)
            {
                return Some(format.clone());
            } else if tiling.eq(&vk::ImageTiling::OPTIMAL)
                && props.optimal_tiling_features.contains(features)
            {
                return Some(format.clone());
            }
        }

        None
    }

    fn has_stencil_component(format: vk::Format) -> bool {
        format.eq(&vk::Format::D32_SFLOAT_S8_UINT) || format.eq(&vk::Format::D24_UNORM_S8_UINT)
    }
}

fn begin_single_time_commands(device: &ash::Device, pool: vk::CommandPool) -> vk::CommandBuffer {
    let ai = vk::CommandBufferAllocateInfo::builder()
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_pool(pool)
        .command_buffer_count(1);

    unsafe {
        let cb = device
            .allocate_command_buffers(&ai)
            .expect("allocating command buffer")
            .first()
            .unwrap()
            .clone();

        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        device
            .begin_command_buffer(cb, &begin_info)
            .expect("Beginning command buyffer");

        cb
    }
}

fn end_single_time_commands(
    device: &ash::Device,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    queue: vk::Queue,
) {
    unsafe {
        device
            .end_command_buffer(command_buffer)
            .expect("Ending buffer")
    };

    let buffers = [command_buffer];
    let submit_infos = [vk::SubmitInfo::builder().command_buffers(&buffers).build()];

    unsafe {
        device
            .queue_submit(queue, &submit_infos, vk::Fence::null())
            .expect("Submitting command buffer");
        device
            .queue_wait_idle(queue)
            .expect("Waiting for queue to become idle after submitting command buffer");

        device.free_command_buffers(command_pool, &buffers);
    }
}

impl Drop for HelloTriangleApplication {
    fn drop(&mut self) {
        self.cleanup_swapchain();

        // This forces the debug config to be dropped
        self.debug_config = None;

        unsafe {
            self.logical_device
                .destroy_sampler(self.texture_sampler, None);
            self.logical_device
                .destroy_image_view(self.texture_image_view, None);
            self.logical_device.destroy_image(self.image, None);
            self.logical_device.free_memory(self.image_memory, None);
            self.logical_device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.logical_device.destroy_buffer(self.vertex_buffer, None);
            self.logical_device
                .free_memory(self.vertex_buffer_memory, None);
            self.logical_device.destroy_buffer(self.index_buffer, None);
            self.logical_device
                .free_memory(self.index_buffer_memory, None);

            for &semaphore in self.image_available_semaphores.iter() {
                self.logical_device.destroy_semaphore(semaphore, None);
            }
            for &semaphore in self.render_complete_semaphores.iter() {
                self.logical_device.destroy_semaphore(semaphore, None);
            }

            for &fence in self.frame_fences.iter() {
                self.logical_device.destroy_fence(fence, None);
            }

            self.logical_device
                .destroy_command_pool(self.command_pool, None);

            self.surface_loader.destroy_surface(self.surface, None);
            self.logical_device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn main() {
    let debug_layers = true;

    let event_loop = EventLoop::new();

    let debug_config = if debug_layers {
        let mut severities = vk::DebugUtilsMessageSeverityFlagsEXT::all();
        severities.bitand_assign(vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE.not());
        Some(debug::Configuration::new(
            severities,
            vulkan_debug_utils_callback,
        ))
    } else {
        None
    };
    let app = HelloTriangleApplication::initialize(&event_loop, debug_config);
    app.run(event_loop);
}
