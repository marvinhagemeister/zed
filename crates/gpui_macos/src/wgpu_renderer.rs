use anyhow::Result;
use cocoa::base::{YES, id};
use foreign_types::ForeignType;
use gpui::{DevicePixels, Scene, Size};
use gpui_wgpu::{GpuContext, WgpuRenderer, WgpuSurfaceConfig};
use metal::{CAMetalLayer, MetalLayer, MetalLayerRef};
use objc::{msg_send, sel, sel_impl};
use raw_window_handle as rwh;
use std::{ffi::c_void, fmt, ptr::NonNull, sync::Arc};

pub(crate) type Context = GpuContext;

#[derive(Clone)]
struct RawMacView {
    native_view: usize,
}

impl fmt::Debug for RawMacView {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RawMacView")
            .field("native_view", &format_args!("{:#x}", self.native_view))
            .finish()
    }
}

impl rwh::HasWindowHandle for RawMacView {
    fn window_handle(&self) -> Result<rwh::WindowHandle<'_>, rwh::HandleError> {
        let native_view =
            NonNull::new(self.native_view as *mut c_void).ok_or(rwh::HandleError::Unavailable)?;
        let handle = rwh::AppKitWindowHandle::new(native_view);
        // SAFETY: `Renderer` is dropped before the NSView stored in MacWindowState.
        Ok(unsafe { rwh::WindowHandle::borrow_raw(handle.into()) })
    }
}

impl rwh::HasDisplayHandle for RawMacView {
    fn display_handle(&self) -> Result<rwh::DisplayHandle<'_>, rwh::HandleError> {
        Ok(rwh::DisplayHandle::appkit())
    }
}

pub(crate) struct Renderer {
    renderer: WgpuRenderer,
    context: Context,
    raw_view: RawMacView,
    layer: MetalLayer,
}

pub(crate) unsafe fn new_renderer(
    context: Context,
    native_window: *mut c_void,
    native_view: *mut c_void,
    bounds: Size<f32>,
    transparent: bool,
) -> Renderer {
    let layer = MetalLayer::new();
    layer.set_opaque(!transparent);
    layer.set_maximum_drawable_count(3);
    unsafe {
        let view = native_view as id;
        let _: () = msg_send![view, setLayer: layer.as_ptr()];
        let _: () = msg_send![view, setWantsLayer: YES];
    }

    let scale_factor: f64 = unsafe { msg_send![native_window as id, backingScaleFactor] };
    let raw_view = RawMacView {
        native_view: native_view as usize,
    };
    let config = WgpuSurfaceConfig {
        size: Size::new(
            DevicePixels((bounds.width * scale_factor as f32).round() as i32),
            DevicePixels((bounds.height * scale_factor as f32).round() as i32),
        ),
        transparent,
        preferred_present_mode: None,
    };
    let renderer = WgpuRenderer::new(context.clone(), &raw_view, config, None)
        .unwrap_or_else(|error| panic!("failed to initialize the GPUI wgpu renderer: {error:#}"));
    Renderer {
        renderer,
        context,
        raw_view,
        layer,
    }
}

impl Renderer {
    pub(crate) fn layer(&self) -> Option<&MetalLayerRef> {
        Some(&self.layer)
    }

    pub(crate) fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer.as_ptr()
    }

    pub(crate) fn sprite_atlas(&self) -> &Arc<gpui_wgpu::WgpuAtlas> {
        self.renderer.sprite_atlas()
    }

    pub(crate) fn set_presents_with_transaction(&mut self, enabled: bool) {
        self.layer.set_presents_with_transaction(enabled);
    }

    pub(crate) fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        self.renderer.update_drawable_size(size);
    }

    pub(crate) fn update_transparency(&mut self, transparent: bool) {
        self.layer.set_opaque(!transparent);
        self.renderer.update_transparency(transparent);
    }

    pub(crate) fn draw(&mut self, scene: &Scene) {
        if self.renderer.device_lost() {
            if let Err(error) = self.renderer.recover(&self.raw_view) {
                log::error!("failed to recover the GPUI graphics device: {error:#}");
                return;
            }
        }
        self.renderer.draw(scene);
    }

    pub(crate) fn gpu_specs(&self) -> gpui::GpuSpecs {
        self.renderer.gpu_specs()
    }

    pub(crate) fn gpu_context(&self) -> Option<Arc<gpui::GpuContext>> {
        self.context
            .borrow()
            .as_ref()
            .map(gpui_wgpu::WgpuContext::application_context)
    }

    pub(crate) fn supports_dual_source_blending(&self) -> bool {
        self.renderer.supports_dual_source_blending()
    }

    pub(crate) fn destroy(&mut self) {
        self.renderer.destroy();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn render_to_image(&mut self, scene: &Scene) -> Result<image::RgbaImage> {
        use gpui::PlatformHeadlessRenderer as _;

        let mut renderer = crate::metal_renderer::MetalHeadlessRenderer::new();
        renderer.render_scene_to_image(scene, self.renderer.viewport_size())
    }
}
