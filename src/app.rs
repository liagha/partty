use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::{Window, WindowId};

use crate::shell::Shell;
use crate::view::View;

#[derive(Default)]
pub struct App {
    window: Option<Arc<Window>>,
    view: Option<View>,
    shell: Option<Shell>,
    inbox: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
}

impl App {
    pub fn run() {
        let event = winit::event_loop::EventLoop::new().unwrap();
        event.set_control_flow(ControlFlow::Poll);
        let mut app = Self::default();
        event.run_app(&mut app).unwrap();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, loop_: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes().with_title("partty");
        let window = Arc::new(loop_.create_window(attrs).unwrap());
        self.view = Some(View::open(window.clone(), loop_));
        let (shell, inbox) = Shell::open();
        self.shell = Some(shell);
        self.inbox = Some(inbox);
        self.window = Some(window);
    }

    fn window_event(&mut self, loop_: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let window = match self.window.as_ref() {
            Some(window) if window.id() == id => window.clone(),
            _ => return,
        };
        match event {
            WindowEvent::CloseRequested => loop_.exit(),
            WindowEvent::Resized(size) => {
                if let Some(view) = self.view.as_mut() {
                    view.resize(size.width, size.height, window.scale_factor() as f32);
                }
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(view) = self.view.as_mut() {
                    let size = window.inner_size();
                    view.resize(size.width, size.height, scale_factor as f32);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Some(view) = self.view.as_mut() {
                    view.draw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _loop_: &ActiveEventLoop) {
        if let Some(inbox) = self.inbox.as_ref() {
            while let Ok(bytes) = inbox.try_recv() {
                eprintln!("pty bytes: {}", bytes.len());
            }
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
