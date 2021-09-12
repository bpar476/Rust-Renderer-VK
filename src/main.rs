use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

const APP_TITLE: &str = "Rust Renderer VK";
const WINDOW_WIDTH: u32 = 800;
const WINDOW_HEIGHT: u32 = 600;

struct HelloTriangleApplication {}

impl HelloTriangleApplication {
    pub fn initialize() -> Self {
        Self {}
    }

    fn init_window(event_loop: &EventLoop<()>) -> winit::window::Window {
        winit::window::WindowBuilder::new()
            .with_title(APP_TITLE)
            .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .build(event_loop)
            .expect("Failed to create window.")
    }

    fn init_vulkan(&mut self) {}

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

    fn cleanup(&mut self) {}

    fn run(&mut self, event_loop: EventLoop<()>) {
        self.init_vulkan();
        self.main_loop(event_loop);
        self.cleanup();
    }
}

fn main() {
    let event_loop = EventLoop::new();
    let _window = HelloTriangleApplication::init_window(&event_loop);
    let mut app = HelloTriangleApplication::initialize();
    app.run(event_loop);
}
