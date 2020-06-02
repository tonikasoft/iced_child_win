mod controls;

use cocoa::appkit::{NSApp, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSPoint, NSRect, NSSize};

use iced_wgpu::{wgpu, Backend, Renderer, Settings, Viewport};
use iced_winit::{futures, program, winit, Debug, Size};

use winit::{
    event::{Event, ModifiersState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::desktop::EventLoopExtDesktop,
    platform::macos::{ActivationPolicy, WindowBuilderExtMacOS, WindowExtMacOS},
};

use controls::Controls;

pub fn main() {
    env_logger::init();

    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(500.0, 400.0));
    let parent_window = unsafe {
        NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
            frame,
            NSWindowStyleMask::NSBorderlessWindowMask | NSWindowStyleMask::NSTitledWindowMask,
            NSBackingStoreType::NSBackingStoreBuffered,
            0,
        )
    };
    // this fixes mouse hover
    unsafe { parent_window.setAcceptsMouseMovedEvents_(1) };

    // Initialize winit
    let mut event_loop = EventLoop::new();
    let window = winit::window::WindowBuilder::new()
        // .with_activation_policy(ActivationPolicy::Prohibited)
        .with_visible(true)
        .build(&event_loop)
        .unwrap();

    unsafe {
        NSWindow::setFrame_display_(window.ns_window() as id, frame, 0);
        let child = window.ns_view() as id;
        // NSView::setFrameSize(child, frame.size);
        // NSView::setFrameOrigin(child, frame.origin);
        parent_window.contentView().addSubview_(child);
    };

    let physical_size = window.inner_size();
    let mut viewport = Viewport::with_physical_size(
        Size::new(physical_size.width, physical_size.height),
        window.scale_factor(),
    );
    let mut modifiers = ModifiersState::default();

    // Initialize wgpu
    let surface = wgpu::Surface::create(&window);
    let (mut device, queue) = futures::executor::block_on(async {
        let adapter = wgpu::Adapter::request(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::Default,
                compatible_surface: Some(&surface),
            },
            wgpu::BackendBit::PRIMARY,
        )
        .await
        .expect("Request adapter");

        adapter
            .request_device(&wgpu::DeviceDescriptor {
                extensions: wgpu::Extensions {
                    anisotropic_filtering: false,
                },
                limits: wgpu::Limits::default(),
            })
            .await
    });

    let format = wgpu::TextureFormat::Bgra8UnormSrgb;

    let mut swap_chain = {
        let size = window.inner_size();

        device.create_swap_chain(
            &surface,
            &wgpu::SwapChainDescriptor {
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Mailbox,
            },
        )
    };
    let mut resized = false;

    // Initialize GUI controls
    let controls = Controls::new();

    // Initialize iced
    let mut debug = Debug::new();
    let mut renderer = Renderer::new(Backend::new(&mut device, Settings::default()));

    let mut state =
        program::State::new(controls, viewport.logical_size(), &mut renderer, &mut debug);

    let mut is_close = false;

    unsafe { parent_window.orderFront_(NSApp()) };

    while !is_close {
        // Run event loop
        // in a real application you would call it inside idle function 
        event_loop.run_return(|event, _, control_flow| {
            match event {
                Event::WindowEvent { event, .. } => {
                    match event {
                        WindowEvent::ModifiersChanged(new_modifiers) => {
                            modifiers = new_modifiers;
                        }
                        WindowEvent::Resized(new_size) => {
                            viewport = Viewport::with_physical_size(
                                Size::new(new_size.width, new_size.height),
                                window.scale_factor(),
                            );

                            resized = true;
                        }
                        WindowEvent::CloseRequested => {
                            is_close = true;
                            *control_flow = ControlFlow::Exit;
                        }

                        _ => {}
                    }

                    // Map window event to iced event
                    if let Some(event) = iced_winit::conversion::window_event(
                        &event,
                        window.scale_factor(),
                        modifiers,
                    ) {
                        state.queue_event(event);
                    }
                }
                Event::MainEventsCleared => {
                    // We update iced
                    let _ = state.update(None, viewport.logical_size(), &mut renderer, &mut debug);

                    // and request a redraw
                    window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    if resized {
                        let size = window.inner_size();

                        swap_chain = device.create_swap_chain(
                            &surface,
                            &wgpu::SwapChainDescriptor {
                                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                                format,
                                width: size.width,
                                height: size.height,
                                present_mode: wgpu::PresentMode::Mailbox,
                            },
                        );
                    }

                    let frame = swap_chain.get_next_texture().expect("Next frame");

                    let mut encoder = device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &frame.view,
                            resolve_target: None,
                            load_op: wgpu::LoadOp::Clear,
                            store_op: wgpu::StoreOp::Store,
                            clear_color: wgpu::Color {
                                r: 1.0,
                                g: 0.5,
                                b: 0.0,
                                a: 1.0,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });

                    // And then iced on top
                    let mouse_interaction = renderer.backend_mut().draw(
                        &mut device,
                        &mut encoder,
                        &frame.view,
                        &viewport,
                        state.primitive(),
                        &debug.overlay(),
                    );

                    // Then we submit the work
                    queue.submit(&[encoder.finish()]);

                    // And update the mouse cursor
                    window.set_cursor_icon(iced_winit::conversion::mouse_interaction(
                        mouse_interaction,
                    ));
                }
                // we use Poll instead of Wait, because we can't pause the thread on Plugin::idle
                // and Plugin::idle does its own optimizations
                _ => *control_flow = ControlFlow::Poll,
            }
        });
    }
}
