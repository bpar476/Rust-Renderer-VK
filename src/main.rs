use core::panic;
use num::{self, range};
use std::ffi::{c_void, CStr, CString};
use std::ops::{BitAndAssign, Not};
use std::os::raw::c_char;
use std::path::Path;
mod debug;
mod instance;
mod util;

use ash::extensions::khr::{Surface, Win32Surface};
use ash::vk::{self, DeviceQueueCreateInfo};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

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
    debug_config: Option<debug::Configuration>,
    physical_device: ash::vk::PhysicalDevice,
    queue_families: QueueFamilyIndices,
    logical_device: ash::Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,

    swapchain_data: SwapChainData,
    swapchain_image_views: Vec<vk::ImageView>,

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
}

impl HelloTriangleApplication {
    pub fn initialize(
        window: &winit::window::Window,
        debug_config: Option<debug::Configuration>,
    ) -> Self {
        let mut debug_config = debug_config;
        let entry = unsafe { ash::Entry::new().unwrap() };

        let instance = HelloTriangleApplication::create_instance(&entry, &debug_config);
        for config in debug_config.iter_mut() {
            let result = config.create_messenger(&entry, &instance);
            if result.is_err() {
                println!("error creating debug messenger: {}", result.unwrap_err())
            }
        }

        // TODO Extract surface creation into module

        // We need a handle to the surface loader so we can call the extension functions
        let (surface_loader, surface) =
            HelloTriangleApplication::create_win32_surface(&entry, &instance, window);

        // TODO extract physical device selection into module
        let physical_device = match HelloTriangleApplication::pick_physical_device(
            &instance,
            &surface_loader,
            &surface,
        ) {
            Some(device) => device,
            None => panic!("No suitable physical device"),
        };

        // Extract device and queues into module
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
            debug_config.is_some(),
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

        let render_pass =
            HelloTriangleApplication::create_render_pass(&logical_device, swapchain_data.format);

        let (graphics_pipeline, pipeline_layout) =
            HelloTriangleApplication::create_graphics_pipeline(
                &logical_device,
                swapchain_data.extent,
                render_pass,
            );

        let swap_chain_frame_buffers = HelloTriangleApplication::create_frame_buffers(
            &logical_device,
            &swapchain_image_views,
            swapchain_data.extent,
            render_pass,
        );

        let command_pool =
            HelloTriangleApplication::create_command_pool(&logical_device, &queue_families);

        let command_buffers = HelloTriangleApplication::create_command_buffers(
            &logical_device,
            command_pool,
            render_pass,
            &swap_chain_frame_buffers,
            swapchain_data.extent,
            graphics_pipeline,
        );

        // TODO: Handle image in flight fences
        let (image_available_semaphores, render_complete_semaphores, frame_fences) =
            HelloTriangleApplication::create_synchronisation_primitives(&logical_device);

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
            queue_families,
            logical_device,
            graphics_queue,
            present_queue,
            swapchain_data,
            swapchain_image_views,
            render_pass,
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
            util::read_vk_string(&properties.device_name[..]).unwrap()
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

    fn create_render_pass(device: &ash::Device, swap_chain_format: vk::Format) -> vk::RenderPass {
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

        let subpass = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&[color_attachment_ref])
            .build();

        // Declare subpass dependencies
        let dependency = vk::SubpassDependency::builder()
            // Implicit subpass that always takes place
            .src_subpass(vk::SUBPASS_EXTERNAL)
            // Our subpass, index 0
            .dst_subpass(0)
            // Operation to wait on
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            // Stage that the operation occurs in
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
            .build();
        let subpass_dependencies = [dependency];

        let attachments = &[color_attachment];
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

    fn create_graphics_pipeline(
        device: &ash::Device,
        swap_chain_extents: vk::Extent2D,
        render_pass: vk::RenderPass,
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

        let vert_shader_module =
            HelloTriangleApplication::create_shader_module(device, &vert_shader_code);
        let frag_shader_module =
            HelloTriangleApplication::create_shader_module(device, &frag_shader_code);

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

        // Describe our vertex layout, the input for the vertex shader
        let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::builder()
            .vertex_binding_descriptions(&[])
            .vertex_attribute_descriptions(&[]);

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

        let dynamic_states = &[vk::DynamicState::VIEWPORT, vk::DynamicState::LINE_WIDTH];
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(dynamic_states);

        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder();
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
        swapchain_extent: vk::Extent2D,
        render_pass: vk::RenderPass,
    ) -> Vec<vk::Framebuffer> {
        // Create a frame bufffer for each swap chain image
        swapchain_image_views
            .iter()
            .map(|&image_view| {
                let attachments = [image_view];

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

    /// Allocates `num_buffers` command buffers to the given command pool on the given device
    fn create_command_buffers(
        device: &ash::Device,
        command_pool: vk::CommandPool,
        render_pass: vk::RenderPass,
        frame_buffers: &Vec<vk::Framebuffer>,
        swap_chain_extent: vk::Extent2D,
        graphics_pipeline: vk::Pipeline,
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

            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];

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
                device.cmd_draw(buffer, 3, 1, 0, 0);

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

    fn draw_frame(&mut self) {
        // TODO: Wait for fences
        let current_frame_fences = [self.frame_fences[self.current_frame]];
        unsafe {
            self.logical_device
                .wait_for_fences(&current_frame_fences, true, u64::MAX)
                .expect("Waiting for frame fence");
        };

        // Request an image from the swap chain. It will signal the given semaphore when the image is ready
        let image_index = unsafe {
            self.swapchain_data
                .loader
                .acquire_next_image(
                    self.swapchain_data.swapchain,
                    u64::MAX,
                    self.image_available_semaphores[self.current_frame],
                    vk::Fence::null(),
                )
                .expect("image index")
                .0 as usize
        };

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

        match present_result {
            Ok(_) => {}
            Err(result) => {
                println!("Presentation did not complete: {:?}", result);
            }
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
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
        // This forces the debug config to be dropped
        self.debug_config = None;

        unsafe {
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

            for &frame_buffer in self.swap_chain_frame_buffers.iter() {
                self.logical_device.destroy_framebuffer(frame_buffer, None)
            }

            self.logical_device
                .destroy_render_pass(self.render_pass, None);
            self.logical_device
                .destroy_pipeline(self.graphics_pipeline, None);
            self.logical_device
                .destroy_pipeline_layout(self.pipeline_layout, None);

            for &image_view in self.swapchain_image_views.iter() {
                self.logical_device.destroy_image_view(image_view, None)
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
    let app = HelloTriangleApplication::initialize(&window, debug_config);
    app.run(event_loop, window);
}
