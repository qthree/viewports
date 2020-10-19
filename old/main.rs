use futures::executor::block_on;
use imgui::{im_str, FontSource, Condition, sys as imgui_sys};
use imgui_wgpu::Renderer;
use imgui_winit_support;
use std::time::Instant;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

fn main() {
    wgpu_subscriber::initialize_default_subscriber(None);

    // Set up window and GPU
    let event_loop = EventLoop::new();

    let (adapter, mut manager, first_id) = {
        let instance = wgpu::Instance::new(wgpu::BackendBit::all());

        let version = env!("CARGO_PKG_VERSION");

        let window = Window::new(&event_loop).unwrap();
        window.set_inner_size(LogicalSize {
            width: 1280.0,
            height: 720.0,
        });
        window.set_outer_position(winit::dpi::PhysicalPosition { x: 0, y: 0 });
        window.set_title(&format!("imgui-wgpu {}", version));
        let size = window.inner_size();

        let surface = unsafe { instance.create_surface(&window) };

        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
        }))
        .unwrap();
        dbg!(adapter.get_info());

        let first_id = window.id();
        let manager = windowing::Manager::from_parts(instance, window, surface);
        (adapter, manager, first_id)
    };

    let window = manager.expect_native_window(first_id);
    let mut hidpi_factor = window.scale_factor();

    let (device, mut queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            shader_validation: false,
        },
        None,
    ))
    .unwrap();

    // Set up dear imgui
    let mut imgui = imgui::Context::create();
    let config_flags =
        imgui_sys::ImGuiConfigFlags_DockingEnable | imgui_sys::ImGuiConfigFlags_ViewportsEnable;
    imgui.io_mut().config_flags |= unsafe { std::mem::transmute(config_flags) }; //imgui::ConfigFlags::DOCKING_ENABLE | imgui::ConfigFlags::VIEWPORTS_ENABLE;
    let backend_flags = imgui_sys::ImGuiBackendFlags_PlatformHasViewports
        | imgui_sys::ImGuiBackendFlags_RendererHasViewports;
    imgui.io_mut().backend_flags |= unsafe { std::mem::transmute(backend_flags) };

    let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    platform.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );
    imgui.set_ini_filename(None);

    let font_size = (13.0 * hidpi_factor) as f32;
    imgui.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(imgui::FontConfig {
            oversample_h: 1,
            pixel_snap_h: true,
            size_pixels: font_size,
            ..Default::default()
        }),
    }]);

    //
    // Set up dear imgui wgpu renderer
    //

    #[cfg(not(feature = "glsl-to-spirv"))]
    let mut renderer = Renderer::new(&mut imgui, &device, &mut queue, windowing::Outlet::format());

    #[cfg(feature = "glsl-to-spirv")]
    let mut renderer =
        Renderer::new_glsl(&mut imgui, &device, &mut queue, windowing::Outlet::format());

    let mut last_frame = Instant::now();
    let mut demo_open = true;

    let mut last_cursor = None;

    let proxy = windowing::register_platform(&mut imgui, &window);
    drop(window);
    manager.maintain_outlets(&device);

    // Event loop
    event_loop.run(move |event, event_loop, control_flow| {
        *control_flow = if cfg!(feature = "metal-auto-capture") {
            ControlFlow::Exit
        } else {
            ControlFlow::Poll
        };

        //dbg!(&event);
        let mut active = manager.with_loop(event_loop);
        match event {
            Event::WindowEvent {
                window_id, event: ref win_event
            } => {
                match win_event {
                    WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                        hidpi_factor = *scale_factor;
                    }
                    WindowEvent::Moved(pos) => {
                        #[cfg(windows)]
                        {
                            let window = active.expect_window_mut(window_id);
                            if *pos == [-32000, -32000].into() {
                                window.minimized = true;
                            } else {
                                window.minimized = false;
                            }
                        }
                    }
                    WindowEvent::Resized(size) => {
                        let window = active.expect_window_mut(window_id);
                        if *size == [0, 0].into() {
                            window.minimized = true;
                        } else {
                            window.minimized = false;
                        }
                        active.make_window_dirty(window_id);
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        let window = active.expect_native_window(window_id);
                        let position = position.cast::<f32>();
                        let winpos = window.outer_position().unwrap().cast::<f32>();
                        imgui.io_mut().mouse_pos = [position.x + winpos.x, position.y + winpos.y];
                    }
                    WindowEvent::MouseInput { state, button, .. } => {
                        use winit::event::MouseButton;
                        let pressed = *state == ElementState::Pressed;
                        let io = imgui.io_mut();
                        match *button {
                            MouseButton::Left => io.mouse_down[0] = pressed,
                            MouseButton::Right => io.mouse_down[1] = pressed,
                            MouseButton::Middle => io.mouse_down[2] = pressed,
                            MouseButton::Other(idx @ 0..=4) => io.mouse_down[idx as usize] = pressed,
                            _ => (),
                        }
                    }
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                state: ElementState::Pressed,
                                ..
                            },
                        ..
                    } | WindowEvent::CloseRequested => {
                        if active.close_native_window(window_id) {
                            *control_flow = ControlFlow::Exit;
                        }
                    }
                    WindowEvent::KeyboardInput {
                        input:
                            KeyboardInput {
                                ..
                            },
                        ..
                    } => {
                        dbg!(&win_event);
                    }
                    _ => {}
                };
            }
            Event::MainEventsCleared => active.expect_native_window(first_id).request_redraw(),
            Event::RedrawEventsCleared => {
                windowing::update_monitors(
                    active.expect_native_window(first_id),
                    //imgui.platform_mut(),
                    &mut imgui,
                    false,
                );

                let delta_s = last_frame.elapsed();
                let now = Instant::now();
                imgui.io_mut().update_delta_time(now - last_frame);
                last_frame = now;

                platform
                    .prepare_frame(imgui.io_mut(), active.expect_native_window(first_id))
                    .expect("Failed to prepare frame");

                proxy.borrow_mut().update(&mut active);
                // NewFrame
                let ui = imgui.frame();

                {
                    let window = imgui::Window::new(im_str!("Hello world"));
                    window
                        .size([300.0, 100.0], Condition::FirstUseEver)
                        .build(&ui, || {
                            ui.text(im_str!("Hello world!"));
                            ui.text(im_str!("This...is...imgui-rs on WGPU!"));
                            ui.separator();
                            let mouse_pos = ui.io().mouse_pos;
                            ui.text(im_str!(
                                "Mouse Position: ({:.1},{:.1})",
                                mouse_pos[0],
                                mouse_pos[1]
                            ));
                        });

                    let window = imgui::Window::new(im_str!("Hello too"));
                    window
                        .size([400.0, 200.0], Condition::FirstUseEver)
                        .position([400.0, 200.0], Condition::FirstUseEver)
                        .build(&ui, || {
                            ui.text(im_str!("Frametime: {:?}", delta_s));
                        });

                    ui.show_demo_window(&mut demo_open);
                }

                if last_cursor != Some(ui.mouse_cursor()) {
                    last_cursor = Some(ui.mouse_cursor());
                    platform.prepare_render(&ui, active.expect_native_window(first_id));
                }

                // Render/EndFrame
                let _ = ui.render();
                proxy.borrow_mut().update(&mut active);

                // UpdatePlatformWindows
                unsafe {
                    imgui_sys::igUpdatePlatformWindows();
                }
                proxy.borrow_mut().update(&mut active);

                //RenderPlatformWindowsDefault
                render_all_vp(
                    &proxy,
                    &mut active,
                    &mut imgui,
                    &device,
                    &queue,
                    &mut renderer,
                );
            }
            _ => (),
        }

        if let Some(window) = active.get_native_window(first_id) {
            platform.handle_event(imgui.io_mut(), window, &event);
        }
    });
}

fn render_all_vp(
    proxy: &windowing::SharedProxy,
    manager: &mut windowing::Manager,
    imgui: &mut imgui::Context,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut imgui_wgpu::Renderer,
) {
    let proxy = proxy.borrow();
    proxy.draw_data(manager, imgui, |window, draw_data| {
        let mut encoder: wgpu::CommandEncoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let frame = match window.get_current_frame(&device) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                eprintln!("minimized");
                return;
            }
            Err(e) => {
                eprintln!("dropped frame: {:?}", e);
                return;
            }
        };
        //dbg!(&frame);

        let clear_color = wgpu::Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        };
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &frame.output.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        renderer
            .render(draw_data, &queue, &device, &mut rpass)
            .expect("Rendering failed");
        drop(rpass);
        queue.submit(Some(encoder.finish()));
    });
}
