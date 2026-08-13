use gpui::{
    App, Bounds, Context, Corners, GpuImage, GpuImageAlphaMode, GpuImageColorEncoding,
    GpuImageSampling, Render, TransformationMatrix, Window, WindowBounds, WindowOptions, canvas,
    div, prelude::*, px, size,
};
use gpui_platform::application;
use std::sync::Arc;

struct GpuImageExample {
    image: Arc<GpuImage>,
}

impl Render for GpuImageExample {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let image = Arc::clone(&self.image);
        div().size_full().p_8().bg(gpui::rgb(0x181818)).child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    window
                        .paint_gpu_image(
                            bounds,
                            image.bounds(),
                            Corners::all(px(16.0)),
                            image,
                            GpuImageSampling::Nearest,
                            TransformationMatrix::unit(),
                        )
                        .expect("the example texture should match the active device epoch");
                },
            )
            .size_full(),
        )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(640.0), px(480.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let context = window
                    .gpu_context()
                    .expect("this example requires GPUI's wgpu compositor");
                let texture = context.create_texture(&wgpu::TextureDescriptor {
                    label: Some("direct_gpu_image_example"),
                    size: wgpu::Extent3d {
                        width: 2,
                        height: 2,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                context.queue().write_texture(
                    texture.texture().as_image_copy(),
                    &[
                        255, 80, 80, 255, 80, 200, 255, 255, 80, 200, 255, 255, 255, 80, 80, 255,
                    ],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(8),
                        rows_per_image: Some(2),
                    },
                    texture.texture().size(),
                );
                let image = GpuImage::new(
                    &context,
                    texture,
                    GpuImageAlphaMode::Opaque,
                    GpuImageColorEncoding::DisplayEncoded,
                )
                .expect("the example texture should be directly sampleable");
                cx.new(|_| GpuImageExample { image })
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
