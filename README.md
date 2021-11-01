# Rust Renderer VK

_trendy marketing name pending_

Rust Renderer VK is a 3D renderer written in Rust using the Vulkan graphics API. It is being written mostly for my own educational purposes but if that changes so willl the readme.

## Learning from this repository

Each commit in this repository is meant to represent one nugget of learning graphics programming with Vulkan. The commit messages should summarise the changes as well as some of the theory.
The commit message will link to the pages used to develop the commit.

You will need to install the Vulkan SDK (linked below) in order to run this project successfully. It provides required validation layers as well as libshaderc used to compile the glsl shaders into SPIR-V at build-time.

## Resources used to develop this project

- [Winit](https://docs.rs/winit/0.25.0/winit/)
- [Ash](https://docs.rs/ash/0.33.3+1.2.191/ash/index.html)
- [Vulkan SDK](https://vulkan.lunarg.com/)
- [Vulkan-tutorial](https://vulkan-tutorial.com/)
- [`vulkan-tutorial-rs`](https://github.com/bwasty/vulkan-tutorial-rs)
- [`vulkan-tutorial-rust](https://github.com/unknownue/vulkan-tutorial-rust)
