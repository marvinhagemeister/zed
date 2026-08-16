use gpui::{
    App, Bounds, Context, Corners, GpuImage, GpuImageAlphaMode, GpuImageColorEncoding,
    GpuImageSampling, Render, TransformationMatrix, Window, WindowBounds, WindowOptions, canvas,
    div, prelude::*, px, size,
};
use gpui_platform::application;
use std::sync::Arc;

struct GpuImageExample {
    images: Vec<Arc<GpuImage>>,
}

impl Render for GpuImageExample {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p_8()
            .gap_2()
            .flex()
            .flex_wrap()
            .bg(gpui::rgb(0x181818))
            .children(self.images.iter().cloned().map(|image| {
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        window
                            .paint_gpu_image(
                                bounds,
                                image.bounds(),
                                Corners::all(px(8.0)),
                                image,
                                GpuImageSampling::Linear,
                                TransformationMatrix::unit(),
                            )
                            .expect("the example texture should match the active device epoch");
                    },
                )
                .w(px(128.0))
                .h(px(128.0))
            }))
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
                let images = (0..12)
                    .map(|index| {
                        let (width, height) = if index == 0 { (2400, 1600) } else { (512, 512) };
                        let texture = context.create_texture(&wgpu::TextureDescriptor {
                            label: Some("direct_gpu_image_example"),
                            size: wgpu::Extent3d {
                                width,
                                height,
                                depth_or_array_layers: 1,
                            },
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: wgpu::TextureFormat::Rgba8Unorm,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::RENDER_ATTACHMENT,
                            view_formats: &[],
                        });
                        let mut encoder = context.device().create_command_encoder(
                            &wgpu::CommandEncoderDescriptor {
                                label: Some("direct_gpu_image_example_encoder"),
                            },
                        );
                        {
                            let view = texture
                                .texture()
                                .create_view(&wgpu::TextureViewDescriptor::default());
                            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                label: Some("direct_gpu_image_example_pass"),
                                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                    view: &view,
                                    resolve_target: None,
                                    ops: wgpu::Operations {
                                        load: wgpu::LoadOp::Clear(wgpu::Color {
                                            r: 0.2 + f64::from(index) * 0.05,
                                            g: 0.6,
                                            b: 0.9,
                                            a: 1.0,
                                        }),
                                        store: wgpu::StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                ..Default::default()
                            });
                        }
                        context.queue().submit([encoder.finish()]);
                        GpuImage::new(
                            &context,
                            texture,
                            GpuImageAlphaMode::Straight,
                            GpuImageColorEncoding::DisplayEncoded,
                        )
                        .expect("the example texture should be directly sampleable")
                    })
                    .collect();
                cx.new(|_| GpuImageExample { images })
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
